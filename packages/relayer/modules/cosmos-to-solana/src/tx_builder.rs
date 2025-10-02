//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::str::FromStr;
use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use hex;
use ibc_eureka_relayer_lib::{
    events::{EurekaEventWithHeight, SolanaEurekaEventWithHeight},
    listener::{cosmos_sdk, solana_eureka},
    tx_builder::TxBuilderService,
    utils::{
        cosmos::{
            tm_create_client_params, tm_update_client_params, TmCreateClientParams,
            TmUpdateClientParams,
        },
        solana_eureka::convert_client_state,
    },
};
use ibc_eureka_utils::light_block::LightBlockExt;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use ibc_proto_eureka::Protobuf;
use prost::Message;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    keccak,
    pubkey::Pubkey,
    signature::Signature,
    sysvar,
    transaction::Transaction,
};
use tendermint_rpc::HttpClient;

use solana_ibc_types::{
    derive_client, derive_client_sequence, derive_ibc_app, derive_ics07_client_state,
    derive_ics07_consensus_state, derive_packet_ack, derive_packet_commitment,
    derive_packet_receipt, derive_router_state, get_instruction_discriminator,
    ics07::{ClientState, ConsensusState, ICS07_INITIALIZE_DISCRIMINATOR},
    MsgAckPacket, MsgRecvPacket, Packet, Payload, UpdateClientMsg,
};

use solana_ibc_constants::{ICS07_TENDERMINT_ID, ICS26_ROUTER_ID};

// /// Default trust level for ICS07 Tendermint light client (1/3)
// const DEFAULT_TRUST_LEVEL_NUMERATOR: u64 = 1;
// const DEFAULT_TRUST_LEVEL_DENOMINATOR: u64 = 3;
//
// /// Maximum allowed clock drift in seconds
// const MAX_CLOCK_DRIFT_SECONDS: u64 = 15;
//
/// Mock proof data for testing purposes
const MOCK_PROOF_DATA: &[u8] = b"mock";

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

/// Parameters for building chunk transactions
struct ChunkTxParams<'a> {
    chunk_data: &'a [u8],
    chain_id: &'a str,
    target_height: u64,
    chunk_index: u8,
    recent_blockhash: solana_sdk::hash::Hash,
}

/// Organized transactions for chunked update client
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ChunkedUpdateTransactions {
    /// Metadata creation transaction (must be submitted first)
    pub metadata_tx: Transaction,
    /// All chunk upload transactions (can be submitted in parallel after metadata)
    pub chunk_txs: Vec<Transaction>,
    /// Final assembly transaction (must be submitted last)
    pub assembly_tx: Transaction,
    /// Total number of chunks
    pub total_chunks: usize,
    /// Target height being updated to
    pub target_height: u64,
}

/// Solana relay transactions including chunked update and packet processing
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SolanaRelayTransactions {
    /// Chunked update client transactions (must be submitted first)
    pub update_client: Option<ChunkedUpdateTransactions>,
    /// Packet relay transactions (submitted after client update)
    pub packet_txs: Vec<Transaction>,
}

/// Helper to derive header chunk PDA
fn derive_header_chunk(
    submitter: &Pubkey,
    chain_id: &str,
    height: u64,
    chunk_index: u8,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            b"header_chunk",
            submitter.as_ref(),
            chain_id.as_bytes(),
            &height.to_le_bytes(),
            &[chunk_index],
        ],
        program_id,
    )
}

/// Helper to derive header metadata PDA
fn derive_header_metadata(
    submitter: &Pubkey,
    chain_id: &str,
    height: u64,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            b"header_metadata",
            submitter.as_ref(),
            chain_id.as_bytes(),
            &height.to_le_bytes(),
        ],
        program_id,
    )
}

/// Parameters for building a `RecvPacket` instruction
struct RecvPacketParams<'a> {
    sequence: u64,
    source_client: &'a str,
    destination_client: &'a str,
    payloads: &'a [Vec<u8>],
    timeout_timestamp: u64,
}

/// Parameters for building an `AckPacket` instruction
struct AckPacketParams<'a> {
    sequence: u64,
    source_client: &'a str,
    destination_client: &'a str,
    acknowledgement: &'a [Vec<u8>],
    proof_height: u64,
}

/// Internal representation of IBC events from Cosmos
#[derive(Debug, Clone)]
enum CosmosIbcEvent {
    /// A packet was sent from Cosmos
    SendPacket {
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Vec<u8>>,
        timeout_timestamp: u64,
    },
    /// An acknowledgement was written/received
    AcknowledgePacket {
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Vec<u8>>,
        timeout_timestamp: u64,
        acknowledgements: Vec<Vec<u8>>,
    },
    /// A packet timed out
    TimeoutPacket {
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Vec<u8>>,
        timeout_timestamp: u64,
    },
}

/// The `TxBuilder` produces Solana transactions based on events from Cosmos SDK.
pub struct TxBuilder {
    /// The source chain listener for Cosmos SDK.
    pub src_listener: cosmos_sdk::ChainListener,
    /// The Solana RPC client (wrapped in Arc since `RpcClient` doesn't implement Clone in 2.0).
    /// The target chain listener for Solana.
    pub target_listener: solana_eureka::ChainListener,
    /// The Solana ICS07 program ID.
    pub solana_ics07_router_program_id: Pubkey,
    /// The fee payer address for transactions.
    pub fee_payer: Pubkey,
}

impl TxBuilder {
    /// Build instruction for creating a client
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
    fn build_create_client_instruction(
        &self,
        chain_id: &str,
        latest_height: u64,
        client_state: &ClientState,
        consensus_state: &ConsensusState,
    ) -> Result<Instruction> {
        // Derive PDAs
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, &self.solana_ics07_program_id);
        let (consensus_state_pda, _) = derive_ics07_consensus_state(
            &client_state_pda,
            latest_height,
            &self.solana_ics07_program_id,
        );

        tracing::debug!("Client state PDA: {}", client_state_pda);
        tracing::debug!("Consensus state PDA: {}", consensus_state_pda);

        // Build accounts for the instruction
        let accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(consensus_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        // Use the correct ICS07 initialize discriminator
        let discriminator = ICS07_INITIALIZE_DISCRIMINATOR;

        // Serialize instruction data using Anchor format
        let mut instruction_data = Vec::new();

        // Add discriminator
        instruction_data.extend_from_slice(&discriminator);

        // Serialize parameters in order: chain_id, latest_height, client_state, consensus_state
        instruction_data.extend_from_slice(&chain_id.try_to_vec()?);
        instruction_data.extend_from_slice(&latest_height.try_to_vec()?);
        instruction_data.extend_from_slice(&client_state.try_to_vec()?);
        instruction_data.extend_from_slice(&consensus_state.try_to_vec()?);

        tracing::debug!("Instruction data length: {} bytes", instruction_data.len());

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: instruction_data,
        })
    }

    /// Creates a new `TxBuilder`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to parse program IDs
    pub fn new(
        source_tm_client: HttpClient,
        solana_client: Arc<RpcClient>,
        solana_ics26_program_id: Pubkey,
        solana_ics07_program_id: Pubkey,
        fee_payer: Pubkey,
    ) -> Result<Self> {
        Ok(Self {
            source_tm_client,
            solana_client,
            solana_ics26_program_id,
            solana_ics07_program_id: Pubkey::from_str(ICS07_TENDERMINT_ID)
                .map_err(|e| anyhow::anyhow!("Invalid ICS07 program ID: {e}"))?,
            fee_payer,
        })
    }

    /// Build Solana relay transactions with optional chunked update client
    ///
    /// Returns separate transactions for the chunked update client and packet relaying.
    /// The update client transactions must be submitted first in the correct order:
    /// 1. First chunk transaction
    /// 2. Parallel chunk transactions (can be submitted in parallel)
    /// 3. Assembly transaction
    /// 4. Packet relay transactions
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client transactions
    /// - Failed to build packet instructions
    pub async fn build_solana_relay_txs_with_options(
        &self,
        client_id: String,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
        skip_update_client: bool,
        proof_height: Option<u64>,
    ) -> Result<SolanaRelayTransactions> {
        // Build update client if needed
        let update_client =
            if !skip_update_client && (!src_events.is_empty() || !target_events.is_empty()) {
                tracing::info!("Building update client transactions");
                Some(
                    self.build_chunked_update_client_txs_to_height(client_id, proof_height)
                        .await?,
                )
            } else {
                if skip_update_client {
                    tracing::info!("Skipping update client as requested");
                } else {
                    tracing::info!("No events to process, skipping update client");
                }
                None
            };

        // Build packet transactions
        let packet_txs = self
            .build_packet_transactions(src_events, target_events)
            .await?;

        Ok(SolanaRelayTransactions {
            update_client,
            packet_txs,
        })
    }

    /// Build packet relay transactions from events
    ///
    /// This can be called separately from update client for better control
    /// over transaction submission on Solana.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get blockhash from Solana
    /// - Failed to build packet instructions
    pub async fn build_packet_transactions(
        &self,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
    ) -> Result<Vec<Transaction>> {
        let mut packet_txs = Vec::new();

        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        for event in src_events {
            if let Some(tx) = self
                .build_packet_tx_from_event(event, recent_blockhash)
                .await?
            {
                packet_txs.push(tx);
            }
        }

        for event in target_events {
            tracing::debug!(?event, "Processing timeout event from Solana");
        }

        Ok(packet_txs)
    }

    /// Build a packet transaction from a single event
    async fn build_packet_tx_from_event(
        &self,
        event: CosmosIbcEvent,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<Option<Transaction>> {
        let mut instructions = Vec::new();

        match event {
            CosmosIbcEvent::SendPacket {
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
            } => {
                let recv_packet_ix = self.build_recv_packet_instruction(&RecvPacketParams {
                    sequence,
                    source_client: &source_client,
                    destination_client: &destination_client,
                    payloads: &payloads,
                    timeout_timestamp,
                })?;
                instructions.push(recv_packet_ix);
            }
            CosmosIbcEvent::AcknowledgePacket {
                sequence,
                source_client,
                destination_client,
                acknowledgements,
                proof_height,
            } => {
                tracing::debug!(
                    "Building acknowledgement instruction for sequence {}",
                    sequence
                );
                // The packet was originally sent from Solana (source) to Cosmos (destination)
                // Now we're acknowledging back on Solana
                let ack_packet_ix = self
                    .build_ack_packet_instruction(&AckPacketParams {
                        sequence,
                        source_client: &source_client,
                        destination_client: &destination_client,
                        acknowledgement: &acknowledgements,
                        proof_height,
                    })
                    .await?;
                instructions.push(ack_packet_ix);
            }
            CosmosIbcEvent::TimeoutPacket { .. } => {
                tracing::debug!("Building timeout instruction - not yet implemented");
            }
        }

        if instructions.is_empty() {
            Ok(None)
        } else {
            // Add compute budget instructions to handle complex operations
            // Request 400K compute units (enough for ack packet verification)
            let compute_budget_ix =
                solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
                    400_000,
                );

            // Add priority fee to ensure transaction gets processed
            let priority_fee_ix =
                solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(1000);

            // Prepend compute budget instructions
            let mut all_instructions = vec![compute_budget_ix, priority_fee_ix];
            all_instructions.extend(instructions);

            let mut tx = Transaction::new_with_payer(&all_instructions, Some(&self.fee_payer));
            tx.message.recent_blockhash = recent_blockhash;
            Ok(Some(tx))
        }
    }

    /// Build instruction for `RecvPacket` on Solana
    ///
    /// # Errors
    ///
    /// Returns an error if packet data cannot be serialized
    fn build_recv_packet_instruction(&self, params: &RecvPacketParams) -> Result<Instruction> {
        // Build the packet structure (IBC v2)
        // For now, we'll handle single payload case (ICS20 transfer)
        // TODO: Handle multiple payloads properly
        let payloads = if params.payloads.is_empty() {
            vec![]
        } else {
            // Extract the first payload and assume it's an ICS20 transfer
            vec![Payload {
                source_port: "transfer".to_string(), // Default ICS20 port
                dest_port: "transfer".to_string(),
                version: "ics20-1".to_string(),
                encoding: "json".to_string(),
                value: params.payloads[0].clone(),
            }]
        };

        // Get dest_port for PDA derivation before moving packet
        let dest_port = if payloads.is_empty() {
            "transfer".to_string()
        } else {
            payloads[0].dest_port.clone()
        };

        let packet = Packet {
            sequence: params.sequence,
            source_client: params.source_client.to_string(),
            dest_client: params.destination_client.to_string(),
            timeout_timestamp: i64::try_from(params.timeout_timestamp)
                .map_err(|e| anyhow::anyhow!("Invalid timeout timestamp: {e}"))?,
            payloads,
        };

        // Create the message with mock proofs for now
        let msg = MsgRecvPacket {
            packet,
            proof_commitment: MOCK_PROOF_DATA.to_vec(), // Mock proof for testing
            proof_height: 1,                            // Mock height for testing
        };

        // Derive all required PDAs
        let (router_state, _) = derive_router_state(&self.solana_ics26_program_id);
        let (ibc_app, _) = derive_ibc_app(&dest_port, &self.solana_ics26_program_id);
        let (client_sequence, _) =
            derive_client_sequence(params.destination_client, &self.solana_ics26_program_id);
        let (packet_receipt, _) = derive_packet_receipt(
            params.destination_client,
            params.sequence,
            &self.solana_ics26_program_id,
        );
        let (packet_ack, _) = derive_packet_ack(
            params.destination_client,
            params.sequence,
            &self.solana_ics26_program_id,
        );
        let (client, _) = derive_client(params.destination_client, &self.solana_ics26_program_id);

        // For light client verification, we also need ICS07 accounts
        let (client_state, _) =
            derive_ics07_client_state(params.source_client, &self.solana_ics07_program_id);

        // Query the latest height for the client
        let latest_height = self.query_client_latest_height(params.source_client)?;

        let (consensus_state, _) = derive_ics07_consensus_state(
            &client_state,
            latest_height,
            &self.solana_ics07_program_id,
        );

        // Build accounts list
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

    /// Build instruction for acknowledging a packet on Solana
    ///
    /// # Errors
    ///
    /// Returns an error if packet data cannot be serialized
    async fn build_ack_packet_instruction(
        &self,
        params: &AckPacketParams<'_>,
    ) -> Result<Instruction> {
        tracing::info!(
            "Building ack packet instruction for packet from {} to {}, sequence {}",
            params.source_client,
            params.destination_client,
            params.sequence
        );

        // Build the acknowledgement data
        let acknowledgement = if params.acknowledgement.is_empty() {
            vec![]
        } else {
            // For now, handle single acknowledgement payload
            params.acknowledgement[0].clone()
        };

        // Build the packet structure that matches what was originally sent
        // The packet was sent FROM Solana TO Cosmos, so:
        // - source_client is the Cosmos client ID on Solana (cosmoshub-1)
        // - dest_client is the Solana client ID on Cosmos (08-wasm-0)
        let packet = Packet {
            sequence: params.sequence,
            source_client: params.source_client.to_string(),
            dest_client: params.destination_client.to_string(),
            timeout_timestamp: 0, // Not needed for ack
            payloads: vec![Payload {
                source_port: "transfer".to_string(),
                dest_port: "transfer".to_string(),
                version: "ics20-1".to_string(),
                encoding: "json".to_string(),
                value: vec![], // Empty for ack - just need packet metadata
            }],
        };

        // Query the actual proof from Cosmos for the acknowledgment
        // For a packet sent from Solana (source) to Cosmos (dest):
        // - source_client = the Cosmos client on Solana (e.g., "cosmoshub-1")
        // - destination_client = the Solana client on Cosmos (e.g., "08-wasm-0")
        // Use the packet's ack_commitment_path method for consistency
        let ack_path = packet.ack_commitment_path();

        // tracing::info!("=== DEBUGGING ACK PATH ===");
        // tracing::info!("Packet flow: Solana -> Cosmos (now acknowledging back to Solana)");
        // tracing::info!("Source client (Cosmos on Solana): {}", params.source_client);
        // tracing::info!(
        //     "Dest client (Solana on Cosmos): {}",
        //     params.destination_client
        // );
        // tracing::info!("Sequence: {}", params.sequence);
        // tracing::info!("Proof height: {}", params.proof_height);
        // tracing::info!("Expected ack path (IBC v2 Eureka): destClient + 0x03 + sequence");
        // tracing::info!("Constructed path: {:?}", ack_path);
        // tracing::info!("Path as hex: {}", hex::encode(&ack_path));
        // tracing::info!("Path as string: {}", String::from_utf8_lossy(&ack_path));
        // tracing::info!("Path breakdown:");
        // tracing::info!(
        //     "  - Client ID bytes: {} ({})",
        //     hex::encode(params.destination_client.as_bytes()),
        //     params.destination_client
        // );
        // tracing::info!("  - Separator: 0x03");
        // tracing::info!(
        //     "  - Sequence big-endian: {}",
        //     hex::encode(&params.sequence.to_be_bytes())
        // );

        // Acknowledgment is written at the NEXT height after the packet is received
        // Cosmos SDK state model: ack written at height N is provable at height N+1
        // But the proof at height N+1 proves against the app hash from height N
        let query_height = params.proof_height + 1;

        tracing::info!(
            "Querying acknowledgment proof at height {} (event height {} + 1)",
            query_height,
            params.proof_height
        );
        tracing::info!(
            "Will verify proof against consensus state at height {}",
            params.proof_height
        );

        // FIXME: wrong???? Query the acknowledgment COMMITMENT from Cosmos chain
        let (commitment_value, merkle_proof) = self
            .source_tm_client
            .prove_path(&[b"ibc".to_vec(), ack_path.clone()], query_height)
            .await?;

        if commitment_value.is_empty() {
            tracing::error!("No acknowledgment commitment found at expected IBC v2 path");
            tracing::error!("Path: {}", String::from_utf8_lossy(&ack_path));
            tracing::error!("Path hex: {}", hex::encode(&ack_path));
            tracing::error!("Queried at height: {} (proof_height + 1)", query_height);
            return Err(anyhow::anyhow!(
                "Acknowledgment commitment not found on chain"
            ));
        }

        tracing::info!(
            "✓ Found acknowledgment commitment at IBC v2 path (value: {} bytes)",
            commitment_value.len()
        );
        tracing::info!("Commitment value (hex): {}", hex::encode(&commitment_value));

        // The acknowledgement we have from the event should hash to this commitment
        tracing::info!(
            "Acknowledgment from event (hex): {}",
            hex::encode(&acknowledgement)
        );

        // IBC v2 commitment: sha256_hash(0x02 + sha256_hash(ack))
        // For single payload, it's: sha256(0x02 + sha256(acknowledgement))
        use sha2::{Digest, Sha256};

        // First hash the acknowledgement
        let mut inner_hasher = Sha256::new();
        inner_hasher.update(&acknowledgement);
        let inner_hash = inner_hasher.finalize();

        // Then compute the commitment with 0x02 prefix
        let mut outer_hasher = Sha256::new();
        outer_hasher.update(&[0x02]); // IBC v2 acknowledgment prefix
        outer_hasher.update(&inner_hash);
        let computed_commitment = outer_hasher.finalize().to_vec();

        if computed_commitment != commitment_value {
            tracing::error!("Acknowledgment commitment mismatch!");
            tracing::error!("Computed: {}", hex::encode(&computed_commitment));
            tracing::error!("Expected: {}", hex::encode(&commitment_value));
            return Err(anyhow::anyhow!(
                "Acknowledgment commitment verification failed"
            ));
        }

        tracing::info!("✓ Acknowledgment commitment verified");

        // Query the actual app hash at the proof height from Cosmos
        let light_block = self
            .source_tm_client
            .get_light_block(Some(params.proof_height))
            .await?;
        let app_hash_at_proof_height = light_block.signed_header.header.app_hash;
        tracing::info!("=== COSMOS STATE AT HEIGHT {} ===", params.proof_height);
        tracing::info!(
            "App hash from Cosmos: {}",
            hex::encode(&app_hash_at_proof_height)
        );
        tracing::info!("Block time: {:?}", light_block.signed_header.header.time);
        tracing::info!(
            "Validators hash: {}",
            hex::encode(&light_block.signed_header.header.validators_hash)
        );

        // Log the merkle proof details before encoding (which consumes it)
        tracing::debug!("Proof structure: {:?}", merkle_proof);

        let proof = merkle_proof.encode_vec();
        tracing::info!("Generated proof: {} bytes", proof.len());

        // Log critical debugging info
        tracing::info!(
            "Proof from height {} will verify against consensus state at height {}",
            query_height,
            params.proof_height
        );

        // Use the original proof_height for verification, not query_height
        // The proof from query_height (N+1) proves against app hash at proof_height (N)
        let msg = MsgAckPacket {
            packet,
            acknowledgement, // Use the acknowledgement from the event
            proof_acked: proof,
            proof_height: params.proof_height, // Use original height for verification
        };

        // Derive PDAs for the packet accounts
        let (router_state, _) = derive_router_state(&self.solana_ics26_program_id);

        // Derive IBC app account (using "transfer" port for ICS20)
        let (ibc_app_pda, _) = derive_ibc_app("transfer", &self.solana_ics26_program_id);

        // Query the IBC app account to get the actual program ID
        let ibc_app_account = self
            .solana_client
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

        // For ack, the packet commitment is stored under the SOURCE client (where packet originated)
        // Need to use the ICS26 client ID for the commitment lookup
        let commitment_client_id = if params.source_client == "cosmoshub-1" {
            "cosmoshub-1"
        } else {
            params.source_client
        };

        let (packet_commitment, _) = derive_packet_commitment(
            commitment_client_id, // Use ICS26 client ID for commitment lookup
            params.sequence,
            &self.solana_ics26_program_id,
        );

        // IMPORTANT: The test setup uses different client IDs:
        // - ICS26 router has "cosmoshub-1" registered as the client
        // - ICS07 Tendermint has the actual chain ID "simd-1" for the client state
        // We need to use the correct ID for each component

        // For ICS26 router operations, use the registered client ID
        let ics26_client_id = if params.source_client == "cosmoshub-1" {
            "cosmoshub-1".to_string()
        } else {
            params.source_client.to_string()
        };

        // For ICS07 Tendermint operations, use the actual chain ID
        let ics07_chain_id = if params.source_client == "cosmoshub-1" {
            "simd-1".to_string() // The actual Cosmos chain ID
        } else {
            params.source_client.to_string()
        };

        tracing::info!(
            "Client ID mapping - ICS26: {}, ICS07: {}",
            ics26_client_id,
            ics07_chain_id
        );

        // Derive the client PDA using ICS26 client ID
        let (client, _) = derive_client(&ics26_client_id, &self.solana_ics26_program_id);

        // For light client verification, use ICS07 accounts with the actual chain ID
        let (client_state, _) =
            derive_ics07_client_state(&ics07_chain_id, &self.solana_ics07_program_id);

        // Use the proof height for the consensus state lookup (NOT query_height)
        // The proof from query_height (N+1) verifies against app hash at proof_height (N)
        let (consensus_state, _) = derive_ics07_consensus_state(
            &client_state,
            params.proof_height, // Use proof_height, not query_height
            &self.solana_ics07_program_id,
        );

        // Build accounts list for ack_packet (order matters!)
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

        // Build instruction data
        let discriminator = get_instruction_discriminator("ack_packet");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    /// NOTE: reuse
    /// Build a create client transaction for Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get genesis block
    /// - Failed to query staking parameters
    /// - Failed to parse chain ID
    /// - Failed to serialize instruction data
    pub async fn build_create_client_tx(&self) -> Result<Transaction> {
        let chain_id = self.source_tm_client.get_light_block(None).await?;

        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, &self.solana_ics07_program_id);
        let (consensus_state_pda, _) = derive_ics07_consensus_state(
            &client_state_pda,
            latest_height,
            &self.solana_ics07_program_id,
        );

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

        tracing::debug!("Instruction data length: {} bytes", instruction_data.len());

        let instruction = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: instruction_data,
        };

        let mut tx = Transaction::new_with_payer(&[instruction], Some(&self.fee_payer));

        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        tx.message.recent_blockhash = recent_blockhash;

        Ok(tx)
    }

    /// Build chunked update client transactions for Solana
    ///
    /// Since Tendermint headers always exceed Solana's transaction size limit,
    /// this method splits the header into chunks and creates multiple transactions.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get latest block from Cosmos
    /// - Failed to serialize header
    /// - Failed to get blockhash from Solana
    /// Build chunked update client transactions to a specific height
    pub async fn build_chunked_update_client_txs_to_height(
        &self,
        client_id: String,
        target_height: u64,
    ) -> Result<ChunkedUpdateTransactions> {
        tracing::info!(
            "Building chunked update client transactions for client {} to height {:?}",
            client_id,
            target_height_override
        );

        let TmUpdateClientParams {
            target_height,
            trusted_height,
            proposed_header,
        } = tm_update_client_params(
            self.client_state(&self.src_listener.chain_id().await?)?,
            tm_client,
            Some(target_height),
        )
        .await?;

        let header_bytes = proposed_header.encode_to_vec();

        let header_commitment = keccak::hash(&header_bytes).0;
        let chunks = Self::split_header_into_chunks(&header_bytes);
        let total_chunks = u8::try_from(chunks.len())
            .map_err(|_| anyhow::anyhow!("Too many chunks: {} exceeds u8 max", chunks.len()))?;

        tracing::info!(
            "Header size: {} bytes, split into {} chunks",
            header_bytes.len(),
            total_chunks
        );

        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        let metadata_tx = self.build_create_metadata_transaction(
            &chain_id,
            target_height,
            total_chunks,
            header_commitment,
            recent_blockhash,
        );

        let chunk_txs =
            self.build_chunk_transactions(&chunks, &chain_id, target_height, recent_blockhash)?;

        let assembly_tx = self.build_assembly_transaction(
            &chain_id,
            target_height,
            trusted_height,
            total_chunks,
            recent_blockhash,
        );

        Ok(ChunkedUpdateTransactions {
            metadata_tx,
            chunk_txs,
            assembly_tx,
            total_chunks: total_chunks as usize,
            target_height,
        })
    }

    fn client_state(&self, chain_id: &str) -> Result<ClientState> {
        let (client_state_pda, _) =
            derive_ics07_client_state(actual_chain_id, &self.solana_ics07_program_id);

        let account = self
            .target_listener
            .client()
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

    fn build_create_metadata_transaction(
        &self,
        chain_id: &str,
        target_height: u64,
        total_chunks: u8,
        header_commitment: [u8; 32],
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Transaction {
        let instruction = self.build_create_metadata_instruction(
            chain_id,
            target_height,
            total_chunks,
            header_commitment,
        );

        let mut tx = Transaction::new_with_payer(&[instruction], Some(&self.fee_payer));
        tx.message.recent_blockhash = recent_blockhash;

        tx
    }

    fn build_chunk_transactions(
        &self,
        chunks: &[Vec<u8>],
        chain_id: &str,
        target_height: u64,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<Vec<Transaction>> {
        let mut chunk_txs = Vec::new();

        for (index, chunk_data) in chunks.iter().enumerate() {
            let chunk_index = u8::try_from(index)
                .map_err(|_| anyhow::anyhow!("Chunk index {} exceeds u8 max", index))?;
            let upload_ix = self.build_upload_header_chunk_instruction(
                chain_id,
                target_height,
                chunk_index,
                chunk_data.to_vec(),
            )?;

            let mut chunk_tx = Transaction::new_with_payer(&[upload_ix], Some(&self.fee_payer));
            chunk_txs.push(chunk_tx);
        }

        Ok(chunk_txs)
    }

    fn build_assembly_transaction(
        &self,
        chain_id: &str,
        target_height: u64,
        trusted_height: u64,
        total_chunks: u8,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Transaction {
        let assembly_instruction = self.build_assemble_and_update_client_instruction(
            chain_id,
            target_height,
            trusted_height,
            total_chunks,
        );

        // Add compute budget instructions to increase the limit
        // Request 1.4M compute units (maximum allowed)
        let compute_budget_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        // Optionally set a priority fee to ensure the transaction gets processed
        let priority_fee_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(1000);

        let mut assembly_tx = Transaction::new_with_payer(
            &[compute_budget_ix, priority_fee_ix, assembly_instruction],
            Some(&self.fee_payer),
        );
        assembly_tx.message.recent_blockhash = recent_blockhash;

        assembly_tx
    }

    fn build_create_metadata_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        total_chunks: u8,
        header_commitment: [u8; 32],
    ) -> Instruction {
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, &self.solana_ics07_program_id);
        let (metadata_pda, _) = derive_header_metadata(
            &self.fee_payer,
            chain_id,
            target_height,
            &self.solana_ics07_program_id,
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

        Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        }
    }

    fn build_upload_header_chunk_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        // Create upload chunk params
        let params = UploadChunkParams {
            chain_id: chain_id.to_string(),
            target_height,
            chunk_index,
            chunk_data,
        };

        // Derive PDAs
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, &self.solana_ics07_program_id);
        let (chunk_pda, _) = derive_header_chunk(
            &self.fee_payer,
            chain_id,
            target_height,
            chunk_index,
            &self.solana_ics07_program_id,
        );

        // Build accounts
        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new_readonly(client_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        // Build instruction data
        let discriminator = get_instruction_discriminator("upload_header_chunk");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&params.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        })
    }

    fn build_assemble_and_update_client_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        trusted_height: u64,
        total_chunks: u8,
    ) -> Instruction {
        // Derive PDAs
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, &self.solana_ics07_program_id);
        let (metadata_pda, _) = derive_header_metadata(
            &self.fee_payer,
            chain_id,
            target_height,
            &self.solana_ics07_program_id,
        );
        let (trusted_consensus_state, _) = derive_ics07_consensus_state(
            &client_state_pda,
            trusted_height,
            &self.solana_ics07_program_id,
        );
        let (new_consensus_state, _) = derive_ics07_consensus_state(
            &client_state_pda,
            target_height,
            &self.solana_ics07_program_id,
        );

        // Build accounts
        let mut accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(metadata_pda, false),
            AccountMeta::new_readonly(trusted_consensus_state, false),
            AccountMeta::new(new_consensus_state, false),
            AccountMeta::new(self.fee_payer, false), // submitter who gets rent back
            AccountMeta::new(self.fee_payer, true),  // payer for new consensus state
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        // Add chunk accounts as remaining accounts
        for chunk_index in 0..total_chunks {
            let (chunk_pda, _) = derive_header_chunk(
                &self.fee_payer,
                chain_id,
                target_height,
                chunk_index,
                &self.solana_ics07_program_id,
            );
            accounts.push(AccountMeta::new(chunk_pda, false));
        }

        let discriminator = get_instruction_discriminator("assemble_and_update_client");

        Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: discriminator.to_vec(),
        }
    }
}

#[async_trait::async_trait]
impl TxBuilderService<CosmosSdk, SolanaEureka> for TxBuilder {
    #[tracing::instrument(skip_all)]
    async fn relay_events(
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

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let mut timeout_msgs = cosmos::target_events_to_timeout_msgs(
            dest_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix.as_secs(),
        );

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix.as_secs(),
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        cosmos::inject_mock_proofs(&mut recv_msgs, &mut ack_msgs, &mut timeout_msgs);

        let all_msgs = timeout_msgs
            .into_iter()
            .map(|m| Any::from_msg(&m))
            .chain(recv_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(ack_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .collect::<Result<Vec<_>, _>>()?;

        let tx_body = TxBody {
            messages: all_msgs,
            ..Default::default()
        };
        Ok(tx_body.encode_to_vec())
    }

    // TODO: reuse code
    #[tracing::instrument(skip_all)]
    async fn create_client(&self) -> Result<Transaction> {
        let chain_id = self.src_listener.chain_id().await?;
        let TmCreateClientParams {
            latest_height,
            client_state,
            consensus_state,
        } = tm_create_client_params(self.src_listener.client()).await?;

        let client_state = convert_client_state(client_state)?;

        let consensus_state = solana_ibc_types::ConsensusState {
            timestamp: consensus_state.timestamp,
            root: consensus_state.root,
            next_validators_hash: consensus_state.next_validators_hash,
        };

        let instruction = self.build_create_client_instruction(
            &chain_id,
            latest_height,
            &client_state,
            &consensus_state,
        )?;

        Ok(tx)
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(&self, chain_id: &str) -> Result<Vec<Transaction>> {
        // Add compute budget instructions to increase the limit
        // Request 1.4M compute units (maximum allowed)
        let compute_budget_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        // Optionally set a priority fee to ensure the transaction gets processed
        let priority_fee_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(1000);

        let mut assembly_tx = Transaction::new_with_payer(
            &[compute_budget_ix, priority_fee_ix, assembly_instruction],
            Some(&self.fee_payer),
        );

        assembly_tx.message.recent_blockhash = recent_blockhash;

        let client_state = self.client_state(chain_id)?;

        let target_light_block = self.src_listener.get_light_block(None).await?;
        let trusted_light_block = self
            .src_listener
            .get_light_block(Some(
                client_state
                    .latest_height
                    .ok_or_else(|| anyhow::anyhow!("No latest height found"))?
                    .revision_height,
            ))
            .await?;

        tracing::info!(
            "Generating tx to update '{}' from height: {} to height: {}",
            chain_id,
            trusted_light_block.height().value(),
            target_light_block.height().value()
        );

        let proposed_header = target_light_block.into_header(&trusted_light_block);

        Ok(TxBody {
            messages: vec![Any::from_msg(&msg)?],
            ..Default::default()
        }
        .encode_to_vec())
    }
}
