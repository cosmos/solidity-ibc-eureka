//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use ibc_eureka_relayer_lib::utils::solana_eureka::convert_client_state_to_sol;
use ibc_eureka_relayer_lib::{
    events::{
        solana::solana_timeout_packet_to_tm_timeout, EurekaEventWithHeight,
        SolanaEurekaEventWithHeight,
    },
    utils::{
        cosmos::{
            self, tm_create_client_params, tm_update_client_params, TmCreateClientParams,
            TmUpdateClientParams,
        },
        solana_eureka::{
            convert_consensus_state, ibc_to_solana_ack_packet, ibc_to_solana_recv_packet,
            target_events_to_timeout_msgs,
        },
    },
};
use prost::Message;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar,
    transaction::Transaction,
};

use solana_ibc_types::{
    derive_client, derive_client_sequence, derive_ibc_app, derive_ics07_client_state,
    derive_ics07_consensus_state, derive_packet_ack, derive_packet_commitment,
    derive_packet_receipt, derive_payload_chunk, derive_proof_chunk, derive_router_state,
    get_instruction_discriminator,
    ics07::{ClientState, ConsensusState, ICS07_INITIALIZE_DISCRIMINATOR},
    MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket, MsgUploadChunk,
};
use tendermint_rpc::{Client as _, HttpClient};

/// Maximum size for a chunk (matches `CHUNK_DATA_SIZE` in Solana program)
const MAX_CHUNK_SIZE: usize = 700;

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
    /// All chunk upload transactions (can be submitted in parallel)
    pub chunk_txs: Vec<Vec<u8>>,
    /// Final assembly transaction (must be submitted last, includes metadata as parameters)
    pub assembly_tx: Vec<u8>,
    /// Total number of chunks
    pub total_chunks: usize,
    /// Target height being updated to
    pub target_height: u64,
}

/// Organized transactions for chunked recv packet
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RecvPacketChunkedTxs {
    /// All chunk upload transactions for payloads and proof
    pub chunk_txs: Vec<Vec<u8>>,
    /// Final recv packet transaction
    pub recv_tx: Vec<u8>,
}

/// Organized transactions for chunked ack packet
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AckPacketChunkedTxs {
    /// All chunk upload transactions for payloads and proof
    pub chunk_txs: Vec<Vec<u8>>,
    /// Final ack packet transaction
    pub ack_tx: Vec<u8>,
}

/// Organized transactions for chunked timeout packet
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TimeoutPacketChunkedTxs {
    /// All chunk upload transactions for payloads and proof
    pub chunk_txs: Vec<Vec<u8>>,
    /// Final timeout packet transaction
    pub timeout_tx: Vec<u8>,
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
    ) -> Result<Self> {
        Ok(Self {
            src_tm_client,
            target_solana_client,
            solana_ics26_program_id,
            solana_ics07_program_id,
            fee_payer,
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

        tracing::info!("Client state PDA: {}", client_state_pda);
        tracing::info!("Consensus state PDA: {}", consensus_state_pda);

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
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one recv packet payload element"
            ));
        };

        let (router_state, _) = derive_router_state(self.solana_ics26_program_id);
        let (ibc_app, _) = derive_ibc_app(&payload.dest_port, self.solana_ics26_program_id);

        // Query the ibc_app account to get the app_program_id (same pattern as ack_packet)
        let ibc_app_account = self
            .target_solana_client
            .get_account(&ibc_app)
            .map_err(|e| anyhow::anyhow!("Failed to get IBC app account: {e}"))?;

        // Parse the IBC app account to get the program ID
        let ibc_app_program = if ibc_app_account.data.len() >= 44 {
            let mut offset = 8; // Skip discriminator

            // Read port_id string length (4 bytes, little-endian)
            let port_len = u32::from_le_bytes(
                ibc_app_account.data[offset..offset + 4]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid port length"))?,
            ) as usize;
            offset += 4;

            // Skip the port string data
            offset += port_len;

            // Read the app_program_id (32 bytes)
            if offset + 32 <= ibc_app_account.data.len() {
                let program_bytes: [u8; 32] = ibc_app_account.data[offset..offset + 32]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid program ID bytes"))?;
                Pubkey::new_from_array(program_bytes)
            } else {
                return Err(anyhow::anyhow!(
                    "IBC app account data too short for program ID"
                ));
            }
        } else {
            return Err(anyhow::anyhow!("Invalid IBC app account data length"));
        };

        // Derive the app state PDA
        let (app_state, _) = Pubkey::find_program_address(
            &[b"app_state", payload.dest_port.as_bytes()],
            &ibc_app_program,
        );

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

        // For ICS07, we need the Cosmos chain ID (the chain being tracked by the light client)
        let (client_state, _) =
            derive_ics07_client_state(chain_id, self.solana_ics07_program_id);

        // Use the proof height for the consensus state lookup
        // The proof verifies against the app_hash at the proof height
        let (consensus_state, _) = derive_ics07_consensus_state(
            client_state,
            msg.proof.height, // Use proof metadata height
            self.solana_ics07_program_id,
        );

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_receipt, false),
            AccountMeta::new(packet_ack, false),
            AccountMeta::new_readonly(ibc_app_program, false), // IBC app program
            AccountMeta::new(app_state, false),                 // IBC app state
            AccountMeta::new_readonly(self.solana_ics26_program_id, false), // Router program
            AccountMeta::new_readonly(self.fee_payer, true), // relayer
            AccountMeta::new(self.fee_payer, true),          // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false), // light client program
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        // Add chunk accounts as remaining_accounts (mutable since they'll be closed)
        for chunk_account in chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
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
    async fn build_ack_packet_instruction(
        &self,
        msg: &MsgAckPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        tracing::info!(
            "Building ack packet instruction for packet from {} to {}, sequence {}",
            msg.packet.source_client,
            msg.packet.dest_client,
            msg.packet.sequence
        );

        let solana_ics26_program_id = self.solana_ics26_program_id;

        let (router_state, _) = derive_router_state(solana_ics26_program_id);

        let (ibc_app_pda, _) = derive_ibc_app("transfer", solana_ics26_program_id);

        let ibc_app_account = self
            .target_solana_client
            .get_account(&ibc_app_pda)
            .map_err(|e| anyhow::anyhow!("Failed to get IBC app account: {e}"))?;

        // The IBC app account data structure:
        // - discriminator (8 bytes)
        // - port_id string (4 bytes length + string data)
        // - app_program_id (32 bytes)
        // - authority (32 bytes)

        // Parse the IBC app account to get the program ID
        let ibc_app_program = if ibc_app_account.data.len() >= 44 {
            // Skip discriminator (8 bytes)
            let mut offset = 8;

            // Read port_id string length (4 bytes, little-endian)
            let port_len = u32::from_le_bytes(
                ibc_app_account.data[offset..offset + 4]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid port length"))?,
            ) as usize;
            offset += 4;

            // Skip the port string data
            offset += port_len;

            // Now read the app_program_id (32 bytes)
            if offset + 32 <= ibc_app_account.data.len() {
                let program_bytes: [u8; 32] = ibc_app_account.data[offset..offset + 32]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid program ID bytes"))?;
                Pubkey::new_from_array(program_bytes)
            } else {
                return Err(anyhow::anyhow!(
                    "IBC app account data too short for program ID"
                ));
            }
        } else {
            return Err(anyhow::anyhow!("Invalid IBC app account data length"));
        };

        tracing::info!("IBC app program ID: {}", ibc_app_program);

        // Derive the app state PDA (using correct seeds: app_state + port_id)
        let (app_state, _) = Pubkey::find_program_address(&[b"app_state", b"transfer"], &ibc_app_program);

        let (packet_commitment, _) = derive_packet_commitment(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        // Derive the router client PDA using the packet's source_client (the ICS26 client on Solana)
        let (client, _) = derive_client(&msg.packet.source_client, solana_ics26_program_id);
        tracing::info!(
            "Router client PDA for '{}': {}",
            msg.packet.source_client,
            client
        );

        // For ICS07, we need the Cosmos chain ID (the chain being tracked by the light client)
        let chain_id = self.chain_id().await?;
        tracing::info!("Cosmos chain ID for ICS07 derivation: {}", chain_id);

        let (client_state, _) = derive_ics07_client_state(&chain_id, self.solana_ics07_program_id);
        tracing::info!("ICS07 client state PDA: {}", client_state);

        // Use the proof height for the consensus state lookup (NOT query_height)
        // The proof from query_height (N+1) verifies against app hash at proof_height (N)
        tracing::info!("=== ACK PACKET CONSENSUS STATE DERIVATION ===");
        tracing::info!("  Proof height from message: {}", msg.proof.height);
        tracing::info!("  Will derive consensus state PDA for height: {}", msg.proof.height);

        let (consensus_state, _) = derive_ics07_consensus_state(
            client_state,
            msg.proof.height, // Use proof metadata height
            self.solana_ics07_program_id,
        );

        tracing::info!("  Consensus state PDA: {}", consensus_state);
        tracing::info!("  This PDA should contain app_hash for height: {}", msg.proof.height);
        tracing::info!("  Proof will be verified against this app_hash");

        let mut accounts = vec![
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

        // Add chunk accounts as remaining_accounts (mutable since they'll be closed)
        for chunk_account in chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

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
    fn build_timeout_packet_instruction(
        &self,
        msg: &MsgTimeoutPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        tracing::info!(
            "Building timeout packet instruction for packet from {} to {}, sequence {}",
            msg.packet.source_client,
            msg.packet.dest_client,
            msg.packet.sequence
        );

        let solana_ics26_program_id = self.solana_ics26_program_id;

        let (router_state, _) = derive_router_state(solana_ics26_program_id);

        let (packet_commitment, _) = derive_packet_commitment(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        let (client, _) = derive_client(&msg.packet.dest_client, solana_ics26_program_id);

        // Build accounts list for timeout_packet
        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(packet_commitment, false), // Will be closed after timeout
            AccountMeta::new_readonly(self.fee_payer, true), // relayer
            AccountMeta::new(self.fee_payer, true),     // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(client, false),
        ];

        // Add chunk accounts as remaining_accounts (mutable since they'll be closed)
        for chunk_account in chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

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

        let client_state = ClientState::try_from_slice(&account.data[8..])
            .or_else(|_| {
                // If try_from_slice fails due to extra bytes, use deserialize which is more lenient
                let mut data = &account.data[8..];
                ClientState::deserialize(&mut data)
            })
            .context("Failed to deserialize client state")?;

        Ok(client_state)
    }

    fn split_into_chunks(data: &[u8]) -> Vec<Vec<u8>> {
        data.chunks(MAX_CHUNK_SIZE).map(<[u8]>::to_vec).collect()
    }

    fn split_header_into_chunks(header_bytes: &[u8]) -> Vec<Vec<u8>> {
        Self::split_into_chunks(header_bytes)
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
        // Add compute budget instructions to increase the limit
        // Request 1.4M compute units (maximum allowed)
        let compute_budget_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        // Optionally set a priority fee to ensure the transaction gets processed
        let priority_fee_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(1000);

        vec![compute_budget_ix, priority_fee_ix]
    }

    fn build_upload_payload_chunk_instruction(
        &self,
        client_id: &str,
        sequence: u64,
        payload_index: u8,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index,
            chunk_index,
            chunk_data,
        };

        let (chunk_pda, _) = derive_payload_chunk(
            self.fee_payer,
            client_id,
            sequence,
            payload_index,
            chunk_index,
            self.solana_ics26_program_id,
        );

        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = get_instruction_discriminator("upload_payload_chunk");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    fn build_upload_proof_chunk_instruction(
        &self,
        client_id: &str,
        sequence: u64,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0, // Not used for proof chunks
            chunk_index,
            chunk_data,
        };

        let (chunk_pda, _) = derive_proof_chunk(
            self.fee_payer,
            client_id,
            sequence,
            chunk_index,
            self.solana_ics26_program_id,
        );

        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = get_instruction_discriminator("upload_proof_chunk");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
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
        let mut data = discriminator.to_vec();

        // Serialize parameters (chain_id, target_height)
        let chain_id_len = u32::try_from(chain_id.len()).expect("chain_id too long");
        data.extend_from_slice(&chain_id_len.to_le_bytes());
        data.extend_from_slice(chain_id.as_bytes());
        data.extend_from_slice(&target_height.to_le_bytes());

        let ix = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        let mut instructions = Self::extend_compute_ix();
        instructions.push(ix);

        self.create_tx_bytes(&instructions)
    }
}

impl TxBuilder {
    /// Build chunked recv packet transactions
    fn build_recv_packet_chunked(
        &self,
        chain_id: &str,
        msg: &MsgRecvPacket,
        payload_data: &[Vec<u8>], // Actual payload data for each payload
        proof_data: &[u8],        // Actual proof data
    ) -> Result<RecvPacketChunkedTxs> {
        let mut chunk_txs = Vec::new();

        for (payload_idx, data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            if payload_idx < msg.payloads.len() && msg.payloads[payload_idx].total_chunks > 0 {
                let chunks = Self::split_into_chunks(data);
                for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                    let chunk_index = u8::try_from(chunk_idx)
                        .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                    let instruction = self.build_upload_payload_chunk_instruction(
                        &msg.packet.dest_client,
                        msg.packet.sequence,
                        payload_index,
                        chunk_index,
                        chunk_data.clone(),
                    )?;

                    chunk_txs.push(self.create_tx_bytes(&[instruction])?);
                }
            }
        }

        // Process proof if it needs chunking (based on metadata)
        if msg.proof.total_chunks > 0 {
            let chunks = Self::split_into_chunks(proof_data);
            for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                let chunk_index = u8::try_from(chunk_idx)
                    .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                let instruction = self.build_upload_proof_chunk_instruction(
                    &msg.packet.dest_client,
                    msg.packet.sequence,
                    chunk_index,
                    chunk_data.clone(),
                )?;

                chunk_txs.push(self.create_tx_bytes(&[instruction])?);
            }
        }

        // Build list of chunk account PDAs for remaining_accounts
        let mut remaining_account_pubkeys = Vec::new();

        // Add payload chunk accounts
        for (payload_idx, _data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            if payload_idx < msg.payloads.len() && msg.payloads[payload_idx].total_chunks > 0 {
                for chunk_idx in 0..msg.payloads[payload_idx].total_chunks {
                    let (chunk_pda, _) = derive_payload_chunk(
                        self.fee_payer,
                        &msg.packet.dest_client,
                        msg.packet.sequence,
                        payload_index,
                        chunk_idx,
                        self.solana_ics26_program_id,
                    );
                    remaining_account_pubkeys.push(chunk_pda);
                }
            }
        }

        // Add proof chunk accounts
        if msg.proof.total_chunks > 0 {
            for chunk_idx in 0..msg.proof.total_chunks {
                let (chunk_pda, _) = derive_proof_chunk(
                    self.fee_payer,
                    &msg.packet.dest_client,
                    msg.packet.sequence,
                    chunk_idx,
                    self.solana_ics26_program_id,
                );
                remaining_account_pubkeys.push(chunk_pda);
            }
        }

        // Build the main recv packet instruction with metadata
        let recv_instruction = self.build_recv_packet_instruction(chain_id, msg, remaining_account_pubkeys)?;

        // Add compute budget instructions to handle expensive proof verification
        let mut instructions = Self::extend_compute_ix();
        instructions.push(recv_instruction);

        let recv_tx = self.create_tx_bytes(&instructions)?;

        Ok(RecvPacketChunkedTxs { chunk_txs, recv_tx })
    }

    /// Build chunked ack packet transactions
    async fn build_ack_packet_chunked(
        &self,
        msg: &MsgAckPacket,
        payload_data: &[Vec<u8>], // Actual payload data for each payload
        proof_data: &[u8],        // Actual proof data
    ) -> Result<AckPacketChunkedTxs> {
        tracing::info!("build_ack_packet_chunked: seq={}, payloads.len={}, proof.total_chunks={}",
            msg.packet.sequence, msg.payloads.len(), msg.proof.total_chunks);

        let mut chunk_txs = Vec::new();
        let mut total_payload_chunk_accounts = 0usize;

        // Process each payload
        for (payload_idx, data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            // Check if payload needs chunking (based on metadata)
            if payload_idx < msg.payloads.len() && msg.payloads[payload_idx].total_chunks > 0 {
                tracing::info!("  Payload {}: total_chunks={}, data_len={}",
                    payload_idx, msg.payloads[payload_idx].total_chunks, data.len());
                let chunks = Self::split_into_chunks(data);
                tracing::info!("    Split into {} chunk transactions", chunks.len());
                for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                    let chunk_index = u8::try_from(chunk_idx)
                        .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                    let instruction = self.build_upload_payload_chunk_instruction(
                        &msg.packet.source_client,
                        msg.packet.sequence,
                        payload_index,
                        chunk_index,
                        chunk_data.clone(),
                    )?;

                    chunk_txs.push(self.create_tx_bytes(&[instruction])?);
                    total_payload_chunk_accounts += 1;
                }
            }
        }

        tracing::info!("  Total payload chunk accounts: {}", total_payload_chunk_accounts);

        let mut total_proof_chunk_accounts = 0usize;

        // Process proof if it needs chunking (based on metadata)
        if msg.proof.total_chunks > 0 {
            let chunks = Self::split_into_chunks(proof_data);
            tracing::info!("  Proof: total_chunks={}, data_len={}, split into {} chunk transactions",
                msg.proof.total_chunks, proof_data.len(), chunks.len());
            for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                let chunk_index = u8::try_from(chunk_idx)
                    .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                let instruction = self.build_upload_proof_chunk_instruction(
                    &msg.packet.source_client,
                    msg.packet.sequence,
                    chunk_index,
                    chunk_data.clone(),
                )?;

                chunk_txs.push(self.create_tx_bytes(&[instruction])?);
                total_proof_chunk_accounts += 1;
            }
        }

        tracing::info!("  Total proof chunk accounts: {}", total_proof_chunk_accounts);
        tracing::info!("  Total chunk upload txs: {}, then 1 final ack_packet tx",
            total_payload_chunk_accounts + total_proof_chunk_accounts);

        // Build list of chunk account PDAs for remaining_accounts
        let mut remaining_account_pubkeys = Vec::new();

        // Add payload chunk accounts
        for (payload_idx, _data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            if payload_idx < msg.payloads.len() && msg.payloads[payload_idx].total_chunks > 0 {
                for chunk_idx in 0..msg.payloads[payload_idx].total_chunks {
                    let (chunk_pda, _) = derive_payload_chunk(
                        self.fee_payer,
                        &msg.packet.source_client,
                        msg.packet.sequence,
                        payload_index,
                        chunk_idx,
                        self.solana_ics26_program_id,
                    );
                    remaining_account_pubkeys.push(chunk_pda);
                }
            }
        }

        // Add proof chunk accounts
        if msg.proof.total_chunks > 0 {
            for chunk_idx in 0..msg.proof.total_chunks {
                let (chunk_pda, _) = derive_proof_chunk(
                    self.fee_payer,
                    &msg.packet.source_client,
                    msg.packet.sequence,
                    chunk_idx,
                    self.solana_ics26_program_id,
                );
                remaining_account_pubkeys.push(chunk_pda);
            }
        }

        tracing::info!("  Adding {} remaining_accounts (chunk PDAs) to ack_packet instruction",
            remaining_account_pubkeys.len());

        // Build the main ack packet instruction with metadata
        let ack_instruction = self.build_ack_packet_instruction(msg, remaining_account_pubkeys).await?;

        // Add compute budget instructions to handle expensive proof verification
        let mut instructions = Self::extend_compute_ix();
        instructions.push(ack_instruction);

        let ack_tx = self.create_tx_bytes(&instructions)?;

        Ok(AckPacketChunkedTxs { chunk_txs, ack_tx })
    }

    /// Build chunked timeout packet transactions
    fn build_timeout_packet_chunked(
        &self,
        msg: &MsgTimeoutPacket,
        payload_data: &[Vec<u8>], // Actual payload data for each payload
        proof_data: &[u8],        // Actual proof data
    ) -> Result<TimeoutPacketChunkedTxs> {
        let mut chunk_txs = Vec::new();

        // Process each payload
        for (payload_idx, data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            // Check if payload needs chunking (based on metadata)
            if payload_idx < msg.payloads.len() && msg.payloads[payload_idx].total_chunks > 0 {
                let chunks = Self::split_into_chunks(data);
                for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                    let chunk_index = u8::try_from(chunk_idx)
                        .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                    let instruction = self.build_upload_payload_chunk_instruction(
                        &msg.packet.source_client,
                        msg.packet.sequence,
                        payload_index,
                        chunk_index,
                        chunk_data.clone(),
                    )?;

                    chunk_txs.push(self.create_tx_bytes(&[instruction])?);
                }
            }
        }

        // Process proof if it needs chunking (based on metadata)
        if msg.proof.total_chunks > 0 {
            let chunks = Self::split_into_chunks(proof_data);
            for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                let chunk_index = u8::try_from(chunk_idx)
                    .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                let instruction = self.build_upload_proof_chunk_instruction(
                    &msg.packet.source_client,
                    msg.packet.sequence,
                    chunk_index,
                    chunk_data.clone(),
                )?;

                chunk_txs.push(self.create_tx_bytes(&[instruction])?);
            }
        }

        // Build list of chunk account PDAs for remaining_accounts
        let mut remaining_account_pubkeys = Vec::new();

        // Add payload chunk accounts
        for (payload_idx, _data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            if payload_idx < msg.payloads.len() && msg.payloads[payload_idx].total_chunks > 0 {
                for chunk_idx in 0..msg.payloads[payload_idx].total_chunks {
                    let (chunk_pda, _) = derive_payload_chunk(
                        self.fee_payer,
                        &msg.packet.source_client,
                        msg.packet.sequence,
                        payload_index,
                        chunk_idx,
                        self.solana_ics26_program_id,
                    );
                    remaining_account_pubkeys.push(chunk_pda);
                }
            }
        }

        // Add proof chunk accounts
        if msg.proof.total_chunks > 0 {
            for chunk_idx in 0..msg.proof.total_chunks {
                let (chunk_pda, _) = derive_proof_chunk(
                    self.fee_payer,
                    &msg.packet.source_client,
                    msg.packet.sequence,
                    chunk_idx,
                    self.solana_ics26_program_id,
                );
                remaining_account_pubkeys.push(chunk_pda);
            }
        }

        // Build the main timeout packet instruction with metadata
        let timeout_instruction = self.build_timeout_packet_instruction(msg, remaining_account_pubkeys)?;

        // Add compute budget instructions to handle expensive proof verification
        let mut instructions = Self::extend_compute_ix();
        instructions.push(timeout_instruction);

        let timeout_tx = self.create_tx_bytes(&instructions)?;

        Ok(TimeoutPacketChunkedTxs {
            chunk_txs,
            timeout_tx,
        })
    }

    /// Build relay transaction from Cosmos events to Solana
    /// Returns a vector of transactions to support chunking
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to convert events to messages
    /// - Failed to build Solana instructions
    /// - Failed to create transaction bytes
    #[tracing::instrument(skip_all)]
    pub async fn relay_events_chunked(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<SolanaEurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<Vec<u8>>> {
        tracing::info!(
            "Relaying chunked events from Cosmos to Solana for client {}",
            dst_client_id
        );

        let chain_id = self.chain_id().await?;
        let solana_client_state = self.cosmos_client_state(&chain_id)?;
        let solana_latest_height = solana_client_state.latest_height.revision_height;

        tracing::info!("=== SOLANA CLIENT STATE ===");
        tracing::info!("  Chain ID: {}", chain_id);
        tracing::info!("  Solana client's latest height: {}", solana_latest_height);
        tracing::info!("  Solana client state: {:?}", solana_client_state);

        // Find the maximum height among all source events
        // This is the height where the latest event (e.g., acknowledgment) was written
        let max_event_height = src_events
            .iter()
            .map(|e| e.height)
            .max()
            .unwrap_or(solana_latest_height);

        tracing::info!("=== EVENT HEIGHTS ===");
        tracing::info!("  Maximum event height from source: {}", max_event_height);
        tracing::info!("  Individual event heights: {:?}", src_events.iter().map(|e| e.height).collect::<Vec<_>>());

        // In Tendermint, data written at height N is committed to the Merkle tree
        // with an app_hash that appears in block N+1's header. Therefore, to prove
        // data written at height N, we need to query at height N and verify against
        // the app_hash from height N+1.
        let proof_height = max_event_height + 1;

        tracing::info!("=== PROOF HEIGHT CALCULATION ===");
        tracing::info!("  Max event height: {}", max_event_height);
        tracing::info!("  Calculated proof_height (max_event + 1): {}", proof_height);
        tracing::info!("  Solana latest height: {}", solana_latest_height);
        tracing::info!("  Solana has consensus state at height: {}", solana_latest_height);

        // Verify Solana has been updated to at least the proof height
        if solana_latest_height < proof_height {
            anyhow::bail!(
                "Solana client is at height {} but need height {} to prove events at height {}. Update Solana client to at least height {} first!",
                solana_latest_height,
                proof_height,
                max_event_height,
                proof_height
            );
        }

        // Use solana_latest_height for proof generation
        // This ensures we use a height where the consensus state actually exists on Solana
        // Solana only stores consensus states at heights where update_client was executed
        let target_height = ibc_proto_eureka::ibc::core::client::v1::Height {
            revision_number: solana_client_state.latest_height.revision_number,
            revision_height: solana_latest_height,
        };

        tracing::info!("=== TARGET HEIGHT FOR PROOF ===");
        tracing::info!("  Using Solana's latest height: {}", target_height.revision_height);
        tracing::info!("  This means: prove_path will query Cosmos at height: {}", target_height.revision_height - 1);
        tracing::info!("  Proof will verify against app_hash from Solana consensus state at height: {}", target_height.revision_height);
        tracing::info!("  Events occurred at height: {}", max_event_height);

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

        tracing::info!("Events to relay to Solana:");
        tracing::info!("  - Timeout messages: {}", timeout_msgs.len());
        tracing::info!("  - Recv messages: {}", recv_msgs.len());
        tracing::info!("  - Ack messages: {}", ack_msgs.len());

        // Log details of ack messages
        for (idx, ack_msg) in ack_msgs.iter().enumerate() {
            if let Some(packet) = &ack_msg.packet {
                tracing::info!(
                    "  Ack #{}: sequence={}, src_client={}, dest_client={}",
                    idx + 1,
                    packet.sequence,
                    packet.source_client,
                    packet.destination_client
                );
            }
        }

        // convert to tm events so we can inject proofs
        let mut timeout_msgs_tm: Vec<_> = timeout_msgs
            .iter()
            .map(|timeout_with_chunks| {
                solana_timeout_packet_to_tm_timeout(
                    timeout_with_chunks.msg.clone(),
                    mock_signer_address.clone(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        cosmos::inject_tendermint_proofs(
            &mut recv_msgs,
            &mut ack_msgs,
            &mut timeout_msgs_tm,
            &self.src_tm_client,
            &target_height,
        )
        .await?;

        // Convert back to Solana format and update proof data
        let mut timeout_msgs_with_chunks = timeout_msgs;
        for (idx, timeout_with_chunks) in timeout_msgs_with_chunks.iter_mut().enumerate() {
            let tm_msg = &timeout_msgs_tm[idx];

            // Update proof chunks with actual proof data
            timeout_with_chunks.proof_chunks = tm_msg.proof_unreceived.clone();

            // Update proof metadata
            let proof_commitment = solana_sdk::keccak::hash(&tm_msg.proof_unreceived).0;
            let proof_total_chunks = u8::try_from(
                tm_msg
                    .proof_unreceived
                    .len()
                    .div_ceil(700) // MAX_CHUNK_SIZE
                    .max(1),
            )
            .context("proof too big to fit in u8")?;

            timeout_with_chunks.msg.proof.commitment = proof_commitment;
            timeout_with_chunks.msg.proof.total_chunks = proof_total_chunks;
        }

        let mut all_txs = Vec::new();
        let chain_id = self.chain_id().await?;

        // Process recv messages with chunking
        for recv_msg in recv_msgs {
            // Convert to Solana format with chunk data
            let recv_with_chunks = ibc_to_solana_recv_packet(recv_msg)?;

            // Build chunked transactions
            let chunked = self.build_recv_packet_chunked(
                &chain_id,
                &recv_with_chunks.msg,
                &recv_with_chunks.payload_chunks,
                &recv_with_chunks.proof_chunks,
            )?;

            // Add all chunks first, then the final recv instruction
            all_txs.extend(chunked.chunk_txs);
            all_txs.push(chunked.recv_tx);
        }

        // Process ack messages with chunking
        for ack_msg in ack_msgs {
            // Convert to Solana format with chunk data
            let ack_with_chunks = ibc_to_solana_ack_packet(ack_msg)?;

            // Build chunked transactions
            let chunked = self
                .build_ack_packet_chunked(
                    &ack_with_chunks.msg,
                    &ack_with_chunks.payload_chunks,
                    &ack_with_chunks.proof_chunks,
                )
                .await?;

            // Add all chunks first, then the final ack instruction
            all_txs.extend(chunked.chunk_txs);
            all_txs.push(chunked.ack_tx);
        }

        // Process timeout messages with chunking
        for timeout_with_chunks in timeout_msgs_with_chunks {
            // Build chunked transactions
            let chunked = self.build_timeout_packet_chunked(
                &timeout_with_chunks.msg,
                &timeout_with_chunks.payload_chunks,
                &timeout_with_chunks.proof_chunks,
            )?;

            // Add all chunks first, then the final timeout instruction
            all_txs.extend(chunked.chunk_txs);
            all_txs.push(chunked.timeout_tx);
        }

        Ok(all_txs)
    }

    fn create_tx_bytes(&self, instructions: &[Instruction]) -> Result<Vec<u8>> {
        let mut tx = Transaction::new_with_payer(instructions, Some(&self.fee_payer));

        let recent_blockhash = self
            .target_solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        tx.message.recent_blockhash = recent_blockhash;

        Ok(bincode::serialize(&tx)?)
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
    pub async fn update_client(&self, dst_client_id: String) -> Result<UpdateClientChunkedTxs> {
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
        let chunks = Self::split_header_into_chunks(&header_bytes);
        let total_chunks = u8::try_from(chunks.len())
            .map_err(|_| anyhow::anyhow!("Too many chunks: {} should fit u8", chunks.len()))?;

        tracing::info!(
            "Header size: {} bytes, split into {} chunks",
            header_bytes.len(),
            total_chunks
        );

        let chunk_txs = self.build_chunk_transactions(&chunks, &chain_id, target_height)?;

        let assembly_tx = self.build_assemble_and_update_client_tx(
            &chain_id,
            target_height,
            trusted_height,
            total_chunks,
        )?;

        tracing::info!(
            "Built {} transactions for chunked update client ({} chunks + 1 assembly with metadata as parameters)",
            total_chunks + 1, // chunks + assembly
            total_chunks
        );

        Ok(UpdateClientChunkedTxs {
            chunk_txs,
            assembly_tx,
            total_chunks: total_chunks as usize,
            target_height,
        })
    }
}
