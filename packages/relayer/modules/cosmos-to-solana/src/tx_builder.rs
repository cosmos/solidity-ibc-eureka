//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::str::FromStr;
use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use ibc_eureka_relayer_lib::{
    events::{
        solana::{solana_timeout_packet_to_tm_timeout, tm_timeout_to_solana_timeout_packet},
        EurekaEventWithHeight, SolanaEurekaEventWithHeight,
    },
    utils::{
        cosmos::{
            self, get_latest_tm_heigth, tm_create_client_params, tm_update_client_params,
            TmCreateClientParams, TmUpdateClientParams,
        },
        solana_eureka::{
            convert_client_state_to_ibc, convert_client_state_to_sol, convert_consensus_state,
            ibc_to_solana_ack_packet, ibc_to_solana_recv_packet, target_events_to_timeout_msgs,
        },
    },
};
use prost::Message;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount},
    commitment_config::CommitmentConfig,
    hash::hash,
    instruction::{AccountMeta, Instruction},
    keccak,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    transaction::{Transaction, VersionedTransaction},
};

use crate::constants::{
    ANCHOR_DISCRIMINATOR_SIZE, GMP_ACCOUNT_STATE_SEED, GMP_PORT_ID, JSON_ENCODING,
    PROTOBUF_ENCODING,
};
use crate::proto::{GmpPacketData, SolanaInstruction};

use solana_ibc_types::{
    derive_app_state, derive_client, derive_client_sequence, derive_ibc_app,
    derive_ics07_client_state, derive_ics07_consensus_state, derive_packet_ack,
    derive_packet_commitment, derive_packet_receipt, derive_router_state,
    get_instruction_discriminator,
    ics07::{ClientState, ConsensusState, ICS07_INITIALIZE_DISCRIMINATOR},
    MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket, Payload,
};
use tendermint_rpc::{Client as _, HttpClient};

/// Maximum size for a header chunk (matches `CHUNK_DATA_SIZE` in Solana program)
const MAX_CHUNK_SIZE: usize = 700;

/// Maximum compute units allowed per Solana transaction
const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

/// Priority fee in micro-lamports per compute unit
const DEFAULT_PRIORITY_FEE: u64 = 1000;

/// Parameters for uploading a header chunk (mirrors the Solana program's type)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
struct UploadChunkParams {
    chain_id: String,
    target_height: u64,
    chunk_index: u8,
    chunk_data: Vec<u8>,
}

/// Organized transactions for chunked update client
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UpdateClientChunkedTxs {
    /// Metadata creation transaction (must be submitted first)
    pub metadata_tx: Vec<u8>,
    /// All chunk upload transactions (can be submitted in parallel after metadata)
    pub chunk_txs: Vec<Vec<u8>>,
    /// Final assembly transaction (must be submitted last)
    pub assembly_tx: Vec<u8>,
    /// Total number of chunks
    pub total_chunks: usize,
    /// Target height being updated to
    pub target_height: u64,
}

/// Update client response that can be either chunked or single transaction
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateClientResponse {
    /// Chunked update (metadata + chunks + assembly)
    Chunked(UpdateClientChunkedTxs),
    /// Single transaction update (for mock)
    Single {
        /// The single transaction to submit
        tx: Vec<u8>,
        /// Target height being updated to
        target_height: u64,
    },
}

/// Helper to derive header chunk PDA
fn derive_header_chunk(
    submitter: Pubkey,
    chain_id: &str,
    height: u64,
    chunk_index: u8,
    program_id: Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            b"header_chunk",
            submitter.as_ref(),
            chain_id.as_bytes(),
            &height.to_le_bytes(),
            &[chunk_index],
        ],
        &program_id,
    )
}

/// Helper to derive header metadata PDA
fn derive_header_metadata(
    submitter: Pubkey,
    chain_id: &str,
    height: u64,
    program_id: Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            b"header_metadata",
            submitter.as_ref(),
            chain_id.as_bytes(),
            &height.to_le_bytes(),
        ],
        &program_id,
    )
}

/// The `TxBuilder` produces Solana transactions based on events from Cosmos SDK.
pub struct TxBuilder {
    /// The HTTP client for Cosmos chain.
    pub src_tm_client: HttpClient,
    /// The target Rpc Client for Solana.
    pub target_solana_client: Arc<RpcClient>,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: Pubkey,
    /// The Solana ICS07 program ID.
    pub solana_ics07_program_id: Pubkey,
    /// The fee payer address for transactions.
    pub fee_payer: Pubkey,
    /// Address Lookup Table address for reducing transaction size (optional).
    pub alt_address: Option<Pubkey>,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to parse program IDs
    pub const fn new(
        src_tm_client: HttpClient,
        target_solana_client: Arc<RpcClient>,
        solana_ics07_program_id: Pubkey,
        solana_ics26_program_id: Pubkey,
        fee_payer: Pubkey,
        alt_address: Option<Pubkey>,
    ) -> Result<Self> {
        Ok(Self {
            src_tm_client,
            target_solana_client,
            solana_ics26_program_id,
            solana_ics07_program_id,
            fee_payer,
            alt_address,
        })
    }

    async fn chain_id(&self) -> Result<String> {
        Ok(self
            .src_tm_client
            .latest_block()
            .await?
            .block
            .header
            .chain_id
            .into())
    }

    fn build_create_client_instruction(
        &self,
        chain_id: &str,
        latest_height: u64,
        client_state: &ClientState,
        consensus_state: &ConsensusState,
    ) -> Result<Instruction> {
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, self.solana_ics07_program_id);
        let (consensus_state_pda, _) = derive_ics07_consensus_state(
            client_state_pda,
            latest_height,
            self.solana_ics07_program_id,
        );

        tracing::debug!("Client state PDA: {}", client_state_pda);
        tracing::debug!("Consensus state PDA: {}", consensus_state_pda);

        let accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(consensus_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = ICS07_INITIALIZE_DISCRIMINATOR;

        let mut instruction_data = Vec::new();

        instruction_data.extend_from_slice(&discriminator);

        instruction_data.extend_from_slice(&chain_id.try_to_vec()?);
        instruction_data.extend_from_slice(&latest_height.try_to_vec()?);
        instruction_data.extend_from_slice(&client_state.try_to_vec()?);
        instruction_data.extend_from_slice(&consensus_state.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: instruction_data,
        })
    }

    fn build_recv_packet_instruction(
        &self,
        chain_id: &str,
        msg: &MsgRecvPacket,
    ) -> Result<Instruction> {
        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one recv packet payload element"
            ));
        };

        let dest_port = payload.dest_port.clone();

        let (router_state, _) = derive_router_state(self.solana_ics26_program_id);
        let (ibc_app, _) = derive_ibc_app(&dest_port, self.solana_ics26_program_id);
        let (client_sequence, _) =
            derive_client_sequence(&msg.packet.dest_client, self.solana_ics26_program_id);
        let (packet_receipt, _) = derive_packet_receipt(
            &msg.packet.dest_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (packet_ack, _) = derive_packet_ack(
            &msg.packet.dest_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (client, _) = derive_client(&msg.packet.dest_client, self.solana_ics26_program_id);

        let (client_state, _) =
            derive_ics07_client_state(&msg.packet.source_client, self.solana_ics07_program_id);

        let latest_height = self
            .cosmos_client_state(chain_id)?
            .latest_height
            .revision_height;

        let (consensus_state, _) =
            derive_ics07_consensus_state(client_state, latest_height, self.solana_ics07_program_id);

        // Resolve the actual IBC app program ID for this port
        let ibc_app_program_id = self.resolve_port_program_id(&dest_port)?;

        // Derive the app state account for the resolved IBC app
        let (ibc_app_state, _) = derive_app_state(&dest_port, ibc_app_program_id);

        // Build base accounts list for recv_packet (matches router program's RecvPacket account structure)
        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_receipt, false),
            AccountMeta::new(packet_ack, false),
            AccountMeta::new_readonly(ibc_app_program_id, false), // IBC app program (e.g., ICS27 GMP)
            AccountMeta::new(ibc_app_state, false),               // IBC app state
            AccountMeta::new_readonly(self.solana_ics26_program_id, false), // router program
            AccountMeta::new_readonly(self.fee_payer, true),      // relayer
            AccountMeta::new(self.fee_payer, true),               // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false), // light client program
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        // Parse payload for additional accounts needed for transaction building
        // IMPORTANT: We only extract account information - the payload itself is NOT modified
        // The packet must be forwarded exactly as received (IBC security invariant)
        if let Ok(additional_accounts) = Self::extract_payload_accounts(
            payload,
            &payload.dest_port,
            &msg.packet.source_client,
            &accounts,
        ) {
            tracing::info!(
                "Found {} additional accounts from GMP payload for port {}",
                additional_accounts.len(),
                payload.dest_port
            );
            accounts.extend(additional_accounts);
        } else {
            tracing::debug!(
                "No additional GMP accounts found in payload for port {} (non-GMP or parsing failed)",
                payload.dest_port
            );
        }

        let discriminator = get_instruction_discriminator("recv_packet");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    fn build_ack_packet_instruction(&self, msg: &MsgAckPacket) -> Result<Instruction> {
        let solana_ics26_program_id = self.solana_ics26_program_id;

        let (router_state, _) = derive_router_state(solana_ics26_program_id);

        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one ack packet payload element"
            ));
        };

        let source_port = payload.source_port.clone();

        let (ibc_app_pda, _) = derive_ibc_app(&source_port, solana_ics26_program_id);

        let ibc_app_account = self
            .target_solana_client
            .get_account(&ibc_app_pda)
            .map_err(|e| anyhow::anyhow!("Failed to get IBC app account: {e}"))?;

        if ibc_app_account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for IBCApp account"));
        }

        // Deserialize IBCApp account using borsh (skip discriminator)
        // Use deserialize instead of try_from_slice to handle extra bytes gracefully
        let mut data = &ibc_app_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;

        let ibc_app_program = ibc_app.app_program_id;
        tracing::info!("IBC app program ID: {}", ibc_app_program);

        // Derive the app state PDA using the correct derivation (same as timeout handler)
        let (app_state, _) = derive_app_state(&source_port, ibc_app_program);

        let (packet_commitment, _) = derive_packet_commitment(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        // Derive the client PDA using ICS26 client ID (from packet source_client)
        let (client, _) = derive_client(&msg.packet.source_client, solana_ics26_program_id);

        let (client_state, _) =
            derive_ics07_client_state(&msg.packet.source_client, self.solana_ics07_program_id);

        // Use the proof height for the consensus state lookup (NOT query_height)
        // The proof from query_height (N+1) verifies against app hash at proof_height (N)
        let (consensus_state, _) = derive_ics07_consensus_state(
            client_state,
            msg.proof_height, // Use proof_height, not query_height
            self.solana_ics07_program_id,
        );

        let accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new(packet_commitment, false), // Will be closed after ack
            AccountMeta::new_readonly(ibc_app_program, false), // IBC app program
            AccountMeta::new(app_state, false),         // IBC app state
            AccountMeta::new_readonly(self.solana_ics26_program_id, false), // Router program
            AccountMeta::new_readonly(self.fee_payer, true), // relayer
            AccountMeta::new(self.fee_payer, true),     // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false), // light client program
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        let discriminator = get_instruction_discriminator("ack_packet");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for timeout packet
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
    fn build_timeout_packet_instruction(&self, msg: &MsgTimeoutPacket) -> Result<Instruction> {
        tracing::info!(
            "Building timeout packet instruction for packet from {} to {}, sequence {}",
            msg.packet.source_client,
            msg.packet.dest_client,
            msg.packet.sequence
        );

        let solana_ics26_program_id = self.solana_ics26_program_id;

        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one timeout packet payload element"
            ));
        };

        let source_port = payload.source_port.clone();

        let (router_state, _) = derive_router_state(solana_ics26_program_id);
        let (ibc_app, _) = derive_ibc_app(&source_port, solana_ics26_program_id);

        let (packet_commitment, _) = derive_packet_commitment(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        // Resolve the actual IBC app program ID for this port
        let ibc_app_program_id = self.resolve_port_program_id(&source_port)?;

        // Derive the app state account for the resolved IBC app
        let (ibc_app_state, _) = derive_app_state(&source_port, ibc_app_program_id);

        let (client, _) = derive_client(&msg.packet.source_client, solana_ics26_program_id);

        let (client_state, _) =
            derive_ics07_client_state(&msg.packet.dest_client, self.solana_ics07_program_id);

        // Use the proof height for the consensus state lookup
        let (consensus_state, _) = derive_ics07_consensus_state(
            client_state,
            msg.proof_height,
            self.solana_ics07_program_id,
        );

        // Build accounts list for timeout_packet (must match router's TimeoutPacket account structure)
        let accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(packet_commitment, false), // Will be closed after timeout
            AccountMeta::new_readonly(ibc_app_program_id, false), // IBC app program
            AccountMeta::new(ibc_app_state, false),     // IBC app state
            AccountMeta::new_readonly(self.solana_ics26_program_id, false), // router program
            AccountMeta::new_readonly(self.fee_payer, true), // relayer
            AccountMeta::new(self.fee_payer, true),     // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false), // light client program
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        // Build instruction data
        let discriminator = get_instruction_discriminator("timeout_packet");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: solana_ics26_program_id,
            accounts,
            data,
        })
    }

    /// Fetch Cosmos client state from the light client on Solana.
    /// # Errors
    /// Returns an error if the client state cannot be fetched or decoded.
    fn cosmos_client_state(&self, chain_id: &str) -> Result<ClientState> {
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, self.solana_ics07_program_id);

        let account = self
            .target_solana_client
            .get_account(&client_state_pda)
            .context("Failed to fetch client state account")?;

        let client_state = ClientState::try_from_slice(&account.data[ANCHOR_DISCRIMINATOR_SIZE..])
            .or_else(|_| {
                // If try_from_slice fails due to extra bytes, use deserialize which is more lenient
                let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
                ClientState::deserialize(&mut data)
            })
            .context("Failed to deserialize client state")?;

        Ok(client_state)
    }

    /// Resolve the IBC app program ID for a given port
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch `IBCApp` account
    /// - Failed to deserialize account data
    fn resolve_port_program_id(&self, port_id: &str) -> Result<Pubkey> {
        let (ibc_app_account, _) = derive_ibc_app(port_id, self.solana_ics26_program_id);

        let account = self
            .target_solana_client
            .get_account(&ibc_app_account)
            .map_err(|e| {
                anyhow::anyhow!("Failed to fetch IBCApp account for port '{}': {e}", port_id)
            })?;

        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for IBCApp account"));
        }

        // Deserialize IBCApp account using borsh (skip discriminator)
        // Use deserialize instead of try_from_slice to handle extra bytes gracefully
        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;

        tracing::info!(
            "Resolved port '{}' to program ID: {}",
            port_id,
            ibc_app.app_program_id
        );

        Ok(ibc_app.app_program_id)
    }

    fn split_header_into_chunks(header_bytes: &[u8]) -> Vec<Vec<u8>> {
        header_bytes
            .chunks(MAX_CHUNK_SIZE)
            .map(<[u8]>::to_vec)
            .collect()
    }

    fn build_create_metadata_tx(
        &self,
        chain_id: &str,
        target_height: u64,
        total_chunks: u8,
        header_commitment: [u8; 32],
    ) -> Result<Vec<u8>> {
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, self.solana_ics07_program_id);
        let (metadata_pda, _) = derive_header_metadata(
            self.fee_payer,
            chain_id,
            target_height,
            self.solana_ics07_program_id,
        );

        let accounts = vec![
            AccountMeta::new(metadata_pda, false),
            AccountMeta::new_readonly(client_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = get_instruction_discriminator("create_metadata");
        let mut data = discriminator.to_vec();

        let chain_id_len = u32::try_from(chain_id.len()).expect("chain_id too long");
        data.extend_from_slice(&chain_id_len.to_le_bytes());
        data.extend_from_slice(chain_id.as_bytes());
        data.extend_from_slice(&target_height.to_le_bytes());
        data.push(total_chunks);
        data.extend_from_slice(&header_commitment);

        let instruction = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        self.create_tx_bytes(&[instruction])
    }

    fn build_chunk_transactions(
        &self,
        chunks: &[Vec<u8>],
        chain_id: &str,
        target_height: u64,
    ) -> Result<Vec<Vec<u8>>> {
        let mut chunk_txs = Vec::new();

        for (index, chunk_data) in chunks.iter().enumerate() {
            let chunk_index = u8::try_from(index)
                .map_err(|_| anyhow::anyhow!("Chunk index {} exceeds u8 max", index))?;
            let upload_ix = self.build_upload_header_chunk_instruction(
                chain_id,
                target_height,
                chunk_index,
                chunk_data.clone(),
            )?;

            let chunk_tx = self.create_tx_bytes(&[upload_ix])?;
            chunk_txs.push(chunk_tx);
        }

        Ok(chunk_txs)
    }

    fn extend_compute_ix() -> Vec<Instruction> {
        let compute_budget_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
                MAX_COMPUTE_UNIT_LIMIT,
            );

        let priority_fee_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(
                DEFAULT_PRIORITY_FEE,
            );

        vec![compute_budget_ix, priority_fee_ix]
    }

    fn build_upload_header_chunk_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        let params = UploadChunkParams {
            chain_id: chain_id.to_string(),
            target_height,
            chunk_index,
            chunk_data,
        };

        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, self.solana_ics07_program_id);
        let (chunk_pda, _) = derive_header_chunk(
            self.fee_payer,
            chain_id,
            target_height,
            chunk_index,
            self.solana_ics07_program_id,
        );

        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new_readonly(client_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = get_instruction_discriminator("upload_header_chunk");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&params.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        })
    }

    fn build_assemble_and_update_client_tx(
        &self,
        chain_id: &str,
        target_height: u64,
        trusted_height: u64,
        total_chunks: u8,
    ) -> Result<Vec<u8>> {
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, self.solana_ics07_program_id);
        let (metadata_pda, _) = derive_header_metadata(
            self.fee_payer,
            chain_id,
            target_height,
            self.solana_ics07_program_id,
        );
        let (trusted_consensus_state, _) = derive_ics07_consensus_state(
            client_state_pda,
            trusted_height,
            self.solana_ics07_program_id,
        );
        let (new_consensus_state, _) = derive_ics07_consensus_state(
            client_state_pda,
            target_height,
            self.solana_ics07_program_id,
        );

        let mut accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(metadata_pda, false),
            AccountMeta::new_readonly(trusted_consensus_state, false),
            AccountMeta::new(new_consensus_state, false),
            AccountMeta::new(self.fee_payer, false), // submitter who gets rent back
            AccountMeta::new(self.fee_payer, true),  // payer for new consensus state
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        for chunk_index in 0..total_chunks {
            let (chunk_pda, _) = derive_header_chunk(
                self.fee_payer,
                chain_id,
                target_height,
                chunk_index,
                self.solana_ics07_program_id,
            );
            accounts.push(AccountMeta::new(chunk_pda, false));
        }

        let discriminator = get_instruction_discriminator("assemble_and_update_client");

        let ix = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: discriminator.to_vec(),
        };

        let mut instructions = Self::extend_compute_ix();
        instructions.push(ix);

        self.create_tx_bytes(&instructions)
    }
}

impl TxBuilder {
    /// Build relay transaction from Cosmos events to Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to convert events to messages
    /// - Failed to build Solana instructions
    /// - Failed to create transaction bytes
    #[tracing::instrument(skip_all)]
    pub async fn relay_events(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<SolanaEurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<u8>> {
        tracing::info!(
            "Relaying events from Cosmos to Solana for client {}",
            dst_client_id
        );

        let chain_id = self.chain_id().await?;
        let client_state = self.cosmos_client_state(&chain_id)?;
        let client_state = convert_client_state_to_ibc(client_state)?;
        let target_height = get_latest_tm_heigth(client_state, &self.src_tm_client).await?;

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let slot = self
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::finalized())
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        let timeout_msgs = target_events_to_timeout_msgs(
            dest_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            slot,
            now_since_unix.as_secs(),
        );

        // we don't care about signer address as no cosmos tx will be sent here
        let mock_signer_address = String::new();

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &mock_signer_address,
            now_since_unix.as_secs(),
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        // convert to tm events so we can inject proofs
        let mut timeout_msgs = timeout_msgs
            .clone()
            .into_iter()
            .map(|msg| solana_timeout_packet_to_tm_timeout(msg, mock_signer_address.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        cosmos::inject_tendermint_proofs(
            &mut recv_msgs,
            &mut ack_msgs,
            &mut timeout_msgs,
            &self.src_tm_client,
            &target_height,
        )
        .await?;

        let timeout_msgs: Vec<_> = timeout_msgs
            .clone()
            .into_iter()
            .map(tm_timeout_to_solana_timeout_packet)
            .collect::<Result<Vec<_>, _>>()?;

        let mut instructions = Vec::new();

        let chain_id = self.chain_id().await?;

        for recv_msg in recv_msgs {
            let recv_msg = ibc_to_solana_recv_packet(recv_msg)?;
            let instruction = self.build_recv_packet_instruction(&chain_id, &recv_msg)?;
            instructions.push(instruction);
        }

        for ack_msg in ack_msgs {
            let ack_msg = ibc_to_solana_ack_packet(ack_msg)?;
            let instruction = self.build_ack_packet_instruction(&ack_msg)?;
            instructions.push(instruction);
        }

        for timeout_msg in timeout_msgs {
            let instruction = self.build_timeout_packet_instruction(&timeout_msg)?;
            instructions.push(instruction);
        }

        self.create_tx_bytes(&instructions)
    }

    fn create_tx_bytes(&self, instructions: &[Instruction]) -> Result<Vec<u8>> {
        if instructions.is_empty() {
            anyhow::bail!("No instructions to execute on Solana");
        }

        let recent_blockhash = self.get_recent_blockhash()?;

        self.alt_address.map_or_else(
            || self.create_legacy_tx(instructions, recent_blockhash),
            |alt_address| self.create_v0_tx_with_alt(instructions, recent_blockhash, alt_address),
        )
    }

    fn get_recent_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.target_solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))
    }

    fn create_v0_tx_with_alt(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
        alt_address: Pubkey,
    ) -> Result<Vec<u8>> {
        tracing::info!(
            "Building transaction with Address Lookup Table: {}",
            alt_address
        );

        let addresses = self.fetch_alt_addresses(alt_address)?;

        tracing::info!("ALT contains {} addresses", addresses.len());
        tracing::info!("ALT addresses: {:?}", addresses);

        let alt_account_for_compile = AddressLookupTableAccount {
            key: alt_address,
            addresses,
        };

        let v0_message = self.compile_v0_message_with_alt(
            instructions,
            recent_blockhash,
            alt_account_for_compile,
        )?;

        Self::log_v0_message_stats(&v0_message);

        Self::serialize_v0_transaction(v0_message)
    }

    fn compile_v0_message_with_alt(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
        alt_account: AddressLookupTableAccount,
    ) -> Result<v0::Message> {
        v0::Message::try_compile(
            &self.fee_payer,
            instructions,
            &[alt_account],
            recent_blockhash,
        )
        .map_err(|e| anyhow::anyhow!("Failed to compile v0 message with ALT: {e}"))
    }

    fn serialize_v0_transaction(v0_message: v0::Message) -> Result<Vec<u8>> {
        let num_signatures = v0_message.header.num_required_signatures as usize;
        let versioned_tx = VersionedTransaction {
            signatures: vec![solana_sdk::signature::Signature::default(); num_signatures],
            message: VersionedMessage::V0(v0_message),
        };

        let serialized_tx = bincode::serialize(&versioned_tx)?;
        tracing::warn!(
            "Transaction size: {} bytes (limit: 1232 bytes raw, 1644 bytes encoded)",
            serialized_tx.len()
        );

        Ok(serialized_tx)
    }

    fn fetch_alt_addresses(&self, alt_address: Pubkey) -> Result<Vec<Pubkey>> {
        let alt_account = self
            .target_solana_client
            .get_account(&alt_address)
            .map_err(|e| anyhow::anyhow!("Failed to fetch ALT account {}: {e}", alt_address))?;

        let lookup_table = AddressLookupTable::deserialize(&alt_account.data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize ALT: {e}"))?;

        Ok(lookup_table.addresses.to_vec())
    }

    fn log_v0_message_stats(v0_message: &v0::Message) {
        tracing::info!(
            "Compiled v0 message: {} static accounts, {} ALT accounts",
            v0_message.account_keys.len(),
            v0_message
                .address_table_lookups
                .iter()
                .map(|lookup| lookup.readonly_indexes.len() + lookup.writable_indexes.len())
                .sum::<usize>()
        );

        tracing::info!("Static account keys: {:?}", v0_message.account_keys);
    }

    fn create_legacy_tx(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<Vec<u8>> {
        let mut tx = Transaction::new_with_payer(instructions, Some(&self.fee_payer));
        tx.message.recent_blockhash = recent_blockhash;

        let versioned_tx = VersionedTransaction::from(tx);
        Ok(bincode::serialize(&versioned_tx)?)
    }

    /// Create a new ICS07 Tendermint client on Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch chain ID from Tendermint RPC
    /// - Failed to fetch client creation parameters (latest height, client state, consensus state)
    /// - Failed to convert Tendermint types to Solana types
    /// - Failed to serialize instruction data
    /// - Failed to get recent blockhash from Solana
    #[tracing::instrument(skip_all)]
    pub async fn create_client(&self) -> Result<Vec<u8>> {
        let chain_id = self.chain_id().await?;
        let TmCreateClientParams {
            latest_height,
            client_state: tm_client_state,
            consensus_state: tm_consensus_state,
        } = tm_create_client_params(&self.src_tm_client).await?;

        let client_state = convert_client_state_to_sol(tm_client_state)?;
        let consensus_state = convert_consensus_state(&tm_consensus_state)?;

        let instruction = self.build_create_client_instruction(
            &chain_id,
            latest_height,
            &client_state,
            &consensus_state,
        )?;

        self.create_tx_bytes(&[instruction])
    }

    /// Build chunked update client transactions
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch chain ID or client state from Solana
    /// - Header size requires more than 255 chunks (`u8::MAX`)
    /// - Failed to serialize header or instruction data
    /// - Failed to get recent blockhash from Solana
    /// - Chain ID string is too long for serialization
    #[tracing::instrument(skip_all)]
    pub async fn update_client(&self, dst_client_id: String) -> Result<UpdateClientResponse> {
        let chain_id = self.chain_id().await?;
        let client_state = self.cosmos_client_state(&chain_id)?;

        let TmUpdateClientParams {
            target_height,
            trusted_height,
            proposed_header,
        } = tm_update_client_params(
            client_state.latest_height.revision_height,
            &self.src_tm_client,
            None,
        )
        .await?;

        tracing::info!(
            "Building chunked update client transactions for client {dst_client_id} to height {target_height}",
        );

        let header_bytes = proposed_header.encode_to_vec();

        let header_commitment = keccak::hash(&header_bytes).0;
        let chunks = Self::split_header_into_chunks(&header_bytes);
        let total_chunks = u8::try_from(chunks.len())
            .map_err(|_| anyhow::anyhow!("Too many chunks: {} should fit u8", chunks.len()))?;

        tracing::info!(
            "Header size: {} bytes, split into {} chunks",
            header_bytes.len(),
            total_chunks
        );

        let metadata_tx = self.build_create_metadata_tx(
            &chain_id,
            target_height,
            total_chunks,
            header_commitment,
        )?;

        let chunk_txs = self.build_chunk_transactions(&chunks, &chain_id, target_height)?;

        let assembly_tx = self.build_assemble_and_update_client_tx(
            &chain_id,
            target_height,
            trusted_height,
            total_chunks,
        )?;

        tracing::info!(
            "Built {} transactions for chunked update client (1 metadata + {} chunks + 1 assembly)",
            total_chunks + 2, // metadata + chunks + assembly
            total_chunks
        );

        Ok(UpdateClientResponse::Chunked(UpdateClientChunkedTxs {
            metadata_tx,
            chunk_txs,
            assembly_tx,
            total_chunks: total_chunks as usize,
            target_height,
        }))
    }

    /// Extract additional accounts from payload for transaction building.
    /// IMPORTANT: This function does NOT modify the payload - it only extracts account information.
    /// The payload must be forwarded exactly as received (IBC security invariant).
    ///
    /// # Arguments
    /// * `payload` - The IBC packet payload
    /// * `port_id` - The destination port ID
    /// * `source_client` - The source client ID (used for PDA derivation)
    /// * `existing_accounts` - Existing accounts list (used to extract IBC app program ID)
    ///
    /// # Returns
    /// Vector of additional accounts needed for GMP transaction execution
    ///
    /// # Errors
    /// Returns error if:
    /// - Payload is not a GMP payload
    /// - Failed to decode GMP packet data
    /// - Invalid receiver pubkey
    /// - Missing IBC app program ID in existing accounts
    fn extract_payload_accounts(
        payload: &Payload,
        port_id: &str,
        source_client: &str,
        existing_accounts: &[AccountMeta],
    ) -> Result<Vec<AccountMeta>, anyhow::Error> {
        let mut account_metas = Vec::new();

        // Only process GMP port payloads
        if port_id != GMP_PORT_ID || payload.encoding != PROTOBUF_ENCODING {
            return Err(anyhow::anyhow!("Not a GMP payload"));
        }

        let gmp_packet = GmpPacketData::decode(payload.value.as_slice())
            .map_err(|e| anyhow::anyhow!("Failed to parse GMPPacketData: {e}"))?;

        // Parse receiver as Solana Pubkey (target program)
        let receiver_pubkey = Pubkey::from_str(&gmp_packet.receiver)
            .map_err(|e| anyhow::anyhow!("Invalid receiver pubkey: {e}"))?;

        // Derive GMP account_state PDA
        // Note: We need the IBC app program ID (ICS27 GMP program) to derive the account_state
        // For now, we'll extract it from existing_accounts - it's account index 5 (ibc_app_program)
        let ibc_app_program_id = if existing_accounts.len() > 5 {
            existing_accounts[5].pubkey
        } else {
            return Err(anyhow::anyhow!(
                "Missing ibc_app_program in existing accounts"
            ));
        };

        // Derive account_state PDA: seeds = [b"gmp_account", client_id, sender_hash, salt]
        // The client_id is the source_client from the IBC packet (the Cosmos chain's client)
        // Always hash the sender for consistent PDA derivation regardless of address format
        let sender_hash = hash(gmp_packet.sender.as_bytes()).to_bytes();

        let account_state_seeds = [
            GMP_ACCOUNT_STATE_SEED,
            source_client.as_bytes(),
            &sender_hash,
            gmp_packet.salt.as_slice(),
        ];
        let (account_state_pda, _) =
            Pubkey::find_program_address(&account_state_seeds, &ibc_app_program_id);

        // Add GMP-specific accounts first
        // 1. account_state PDA (writable, created if needed)
        // Note: At transaction level is_signer=false (PDAs cannot sign transactions)
        // But GMP program marks it is_signer=true at CPI instruction level via invoke_signed
        // This allows target programs to verify the call is authorized by the legitimate owner
        account_metas.push(AccountMeta {
            pubkey: account_state_pda,
            is_signer: false, // No keypair at transaction level (PDA signs via invoke_signed in GMP program)
            is_writable: true,
        });

        // 2. target_program (receiver from GMPPacketData)
        account_metas.push(AccountMeta {
            pubkey: receiver_pubkey,
            is_signer: false,
            is_writable: true, // Might be writable depending on use case
        });

        // Parse SolanaInstruction from the inner payload to extract additional accounts
        match SolanaInstruction::decode(gmp_packet.payload.as_slice()) {
            Ok(solana_instruction) => {
                // Extract all accounts from the instruction and add them to the transaction
                // Note: All accounts are added with is_signer=false at transaction level
                // PDAs will be marked as signers by the GMP program via invoke_signed
                for account_meta in &solana_instruction.accounts {
                    let pubkey = Pubkey::try_from(account_meta.pubkey.as_slice())
                        .map_err(|e| anyhow::anyhow!("Invalid pubkey: {e}"))?;

                    // At transaction level, no payload accounts need to sign
                    // The GMP program handles PDA signing via invoke_signed
                    account_metas.push(AccountMeta {
                        pubkey,
                        is_signer: false,
                        is_writable: account_meta.is_writable,
                    });
                }

                Ok(account_metas)
            }
            Err(e) => Err(anyhow::anyhow!(
                "Failed to parse inner SolanaInstruction: {e}"
            )),
        }
    }

    /// Parse payload to extract additional accounts required for execution
    /// This follows the ADR design where all accounts are included in the payload
    ///
    /// # Arguments
    /// * `payload` - The IBC packet payload
    /// * `port_id` - The destination port ID
    ///
    /// # Returns
    /// Vector of additional accounts needed for execution
    ///
    /// # Errors
    /// Returns error if failed to parse payload
    #[allow(dead_code, clippy::unused_self)]
    fn parse_payload_accounts(
        payload: &Payload,
        port_id: &str,
    ) -> Result<Vec<AccountMeta>, anyhow::Error> {
        let mut account_metas = Vec::new();

        if port_id != GMP_PORT_ID {
            tracing::info!("Skipping payload parsing for non-GMP port: {}", port_id);
            return Ok(account_metas);
        }

        match payload.encoding.as_str() {
            PROTOBUF_ENCODING => {
                match Self::parse_solana_instruction_from_payload(&payload.value) {
                    Ok(solana_instruction) => {
                        for account_meta in &solana_instruction.accounts {
                            let pubkey = Pubkey::try_from(account_meta.pubkey.as_slice())
                                .map_err(|e| anyhow::anyhow!("Invalid pubkey in payload: {e}"))?;

                            // At transaction level, no payload accounts need to sign
                            account_metas.push(AccountMeta {
                                pubkey,
                                is_signer: false,
                                is_writable: account_meta.is_writable,
                            });
                        }
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
            JSON_ENCODING => {
                // For JSON encoding, we could parse structured JSON here if needed
                // The payload should contain all required accounts in structured format
                tracing::info!("JSON encoding detected, no additional accounts extracted yet");
            }
            _ => {
                // For other encodings (like raw bytes), no additional parsing
                tracing::info!(
                    "No structured payload parsing for encoding: {}",
                    payload.encoding
                );
            }
        }

        Ok(account_metas)
    }

    /// Parse `SolanaInstruction` from protobuf-encoded payload
    ///
    /// # Arguments
    /// * `payload_data` - The raw payload bytes
    ///
    /// # Returns
    /// Decoded `SolanaInstruction`
    ///
    /// # Errors
    /// Returns error if failed to decode payload
    fn parse_solana_instruction_from_payload(
        payload_data: &[u8],
    ) -> Result<SolanaInstruction, anyhow::Error> {
        match GmpPacketData::decode(payload_data) {
            Ok(gmp_packet) => {
                SolanaInstruction::decode(gmp_packet.payload.as_slice())
                    .map_err(|e| anyhow::anyhow!("Failed to parse inner SolanaInstruction from GMP payload: {e}"))
            }
            Err(e) => {
                SolanaInstruction::decode(payload_data)
                    .map_err(|e2| anyhow::anyhow!("Failed to parse payload as either GMPPacketData or SolanaInstruction: GMP error: {e}, Direct error: {e2}"))
            }
        }
    }
}

/// Mock transaction builder that wraps the real `TxBuilder` but uses simplified `update_client`
pub struct MockTxBuilder {
    /// The underlying real `TxBuilder`
    pub inner: TxBuilder,
}

impl MockTxBuilder {
    /// Creates a new `MockTxBuilder`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to create the underlying `TxBuilder`
    pub fn new(
        src_tm_client: HttpClient,
        target_solana_client: Arc<RpcClient>,
        solana_ics07_program_id: Pubkey,
        solana_ics26_program_id: Pubkey,
        fee_payer: Pubkey,
        alt_address: Option<Pubkey>,
    ) -> Result<Self> {
        Ok(Self {
            inner: TxBuilder::new(
                src_tm_client,
                target_solana_client,
                solana_ics07_program_id,
                solana_ics26_program_id,
                fee_payer,
                alt_address,
            )?,
        })
    }

    /// Build relay transactions with MOCK proofs (4 bytes instead of 600-800 bytes)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to convert events to messages
    /// - Failed to build Solana instructions
    /// - Failed to create transaction bytes
    #[tracing::instrument(skip_all)]
    pub async fn relay_events(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<SolanaEurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<u8>> {
        tracing::info!(
            "Mock relay_events: using MOCK proofs for client {} (4 bytes instead of 600-800 bytes)",
            dst_client_id
        );

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let slot = self
            .inner
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::finalized())
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        let timeout_msgs = target_events_to_timeout_msgs(
            dest_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            slot,
            now_since_unix.as_secs(),
        );

        let mock_signer_address = String::new();

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &mock_signer_address,
            now_since_unix.as_secs(),
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        let mut timeout_msgs = timeout_msgs
            .clone()
            .into_iter()
            .map(|msg| solana_timeout_packet_to_tm_timeout(msg, mock_signer_address.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        // CRITICAL: Inject MOCK proofs instead of real Tendermint proofs
        // This reduces proof size from ~600-800 bytes to just 4 bytes (b"mock")
        tracing::info!("Injecting MOCK proofs (4 bytes each) instead of real Tendermint proofs");
        cosmos::inject_mock_proofs(&mut recv_msgs, &mut ack_msgs, &mut timeout_msgs);

        let timeout_msgs: Vec<_> = timeout_msgs
            .clone()
            .into_iter()
            .map(tm_timeout_to_solana_timeout_packet)
            .collect::<Result<Vec<_>, _>>()?;

        let mut instructions = Vec::new();

        let chain_id = self.inner.chain_id().await?;

        for recv_msg in recv_msgs {
            let recv_msg = ibc_to_solana_recv_packet(recv_msg)?;
            let instruction = self
                .inner
                .build_recv_packet_instruction(&chain_id, &recv_msg)?;
            instructions.push(instruction);
        }

        for ack_msg in ack_msgs {
            let ack_msg = ibc_to_solana_ack_packet(ack_msg)?;
            let instruction = self.inner.build_ack_packet_instruction(&ack_msg)?;
            instructions.push(instruction);
        }

        for timeout_msg in timeout_msgs {
            let instruction = self.inner.build_timeout_packet_instruction(&timeout_msg)?;
            instructions.push(instruction);
        }

        instructions.extend(TxBuilder::extend_compute_ix());

        self.inner.create_tx_bytes(&instructions)
    }

    /// Delegate to real `create_client` logic
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `TxBuilder::create_client` fails
    #[tracing::instrument(skip_all)]
    pub async fn create_client(&self) -> Result<Vec<u8>> {
        tracing::info!("Mock create_client: delegating to real TxBuilder");
        self.inner.create_client().await
    }

    /// Build a normal (non-chunked) mock `update_client` instruction
    fn build_mock_update_client_instruction(&self) -> Result<Instruction> {
        tracing::debug!("Building MOCK update client instruction with minimal data");

        // The mock light client will accept any payload, so we use minimal mock data
        let mock_client_message = b"mock_client_message".to_vec();

        let discriminator = get_instruction_discriminator("update_client");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&mock_client_message.try_to_vec()?);

        tracing::debug!("Mock update client instruction: {} bytes total", data.len());

        // Build with mock accounts - the mock light client doesn't validate these
        Ok(Instruction {
            program_id: self.inner.solana_ics07_program_id,
            accounts: vec![
                AccountMeta::new(self.inner.fee_payer, false), // Mock client state
                AccountMeta::new(self.inner.fee_payer, false), // Mock trusted consensus state
                AccountMeta::new(self.inner.fee_payer, false), // Mock new consensus state
                AccountMeta::new(self.inner.fee_payer, true),  // Payer
                AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            ],
            data,
        })
    }

    /// Build mock update client with a single simple instruction (no chunking)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build mock update client instruction
    /// - Failed to create transaction bytes
    #[tracing::instrument(skip_all)]
    pub async fn update_client(&self, dst_client_id: String) -> Result<UpdateClientResponse> {
        tracing::info!(
            "Building MOCK update client for {} - using single instruction instead of chunked headers",
            dst_client_id
        );

        // Build a single mock update instruction
        let mock_update_ix = self.build_mock_update_client_instruction()?;
        let mock_tx = self.inner.create_tx_bytes(&[mock_update_ix])?;

        // Return as a single transaction (no chunking)
        Ok(UpdateClientResponse::Single {
            tx: mock_tx,
            target_height: 1,
        })
    }
}
