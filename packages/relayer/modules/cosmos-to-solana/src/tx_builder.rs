//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use ibc_eureka_relayer_lib::{
    events::{EurekaEventWithHeight, SolanaEurekaEventWithHeight},
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
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    keccak,
    pubkey::Pubkey,
    sysvar,
    transaction::Transaction,
};

use solana_ibc_types::{
    derive_client, derive_client_sequence, derive_ibc_app, derive_ics07_client_state,
    derive_ics07_consensus_state, derive_packet_ack, derive_packet_commitment,
    derive_packet_receipt, derive_router_state, get_instruction_discriminator,
    ics07::{ClientState, ConsensusState, ICS07_INITIALIZE_DISCRIMINATOR},
    MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket,
};
use tendermint_rpc::{Client as _, HttpClient};

/// Maximum size for a header chunk (matches `CHUNK_DATA_SIZE` in Solana program)
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

        let (router_state, _) = derive_router_state(self.solana_ics26_program_id);
        let (ibc_app, _) = derive_ibc_app(&payload.dest_port, self.solana_ics26_program_id);
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

        let accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_receipt, false),
            AccountMeta::new(packet_ack, false),
            AccountMeta::new_readonly(self.fee_payer, true), // relayer
            AccountMeta::new(self.fee_payer, true),          // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false), // light client program
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

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
    async fn build_ack_packet_instruction(&self, msg: &MsgAckPacket) -> Result<Instruction> {
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

        // Derive the app state PDA
        let (app_state, _) = Pubkey::find_program_address(&[b"state"], &ibc_app_program);

        let (packet_commitment, _) = derive_packet_commitment(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        // Derive the client PDA using ICS26 client ID
        let chain_id = self.chain_id().await?;
        let (client, _) = derive_client(&chain_id, solana_ics26_program_id);

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

        let (router_state, _) = derive_router_state(solana_ics26_program_id);

        let (packet_commitment, _) = derive_packet_commitment(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        let (client, _) = derive_client(&msg.packet.dest_client, solana_ics26_program_id);

        // Build accounts list for timeout_packet
        let accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(packet_commitment, false), // Will be closed after timeout
            AccountMeta::new_readonly(self.fee_payer, true), // relayer
            AccountMeta::new(self.fee_payer, true),     // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(client, false),
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

        let client_state = ClientState::try_from_slice(&account.data[8..])
            .or_else(|_| {
                // If try_from_slice fails due to extra bytes, use deserialize which is more lenient
                let mut data = &account.data[8..];
                ClientState::deserialize(&mut data)
            })
            .context("Failed to deserialize client state")?;

        Ok(client_state)
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
        // Add compute budget instructions to increase the limit
        // Request 1.4M compute units (maximum allowed)
        let compute_budget_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        // Optionally set a priority fee to ensure the transaction gets processed
        let priority_fee_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(1000);

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

        cosmos::inject_tendermint_proofs(
            &mut recv_msgs,
            &mut ack_msgs,
            &mut [],
            &self.src_tm_client,
            &target_height,
        )
        .await?;

        let mut instructions = Vec::new();

        let chain_id = self.chain_id().await?;

        for recv_msg in recv_msgs {
            let recv_msg = ibc_to_solana_recv_packet(recv_msg)?;
            let instruction = self.build_recv_packet_instruction(&chain_id, &recv_msg)?;
            instructions.push(instruction);
        }

        for ack_msg in ack_msgs {
            let ack_msg = ibc_to_solana_ack_packet(ack_msg)?;
            let instruction = self.build_ack_packet_instruction(&ack_msg).await?;
            instructions.push(instruction);
        }

        for timeout_msg in timeout_msgs {
            let instruction = self.build_timeout_packet_instruction(&timeout_msg)?;
            instructions.push(instruction);
        }

        self.create_tx_bytes(&instructions)
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

        Ok(UpdateClientChunkedTxs {
            metadata_tx,
            chunk_txs,
            assembly_tx,
            total_chunks: total_chunks as usize,
            target_height,
        })
    }
}
