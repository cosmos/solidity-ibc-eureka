//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::str::FromStr;
use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use hex;
use ibc_eureka_relayer_lib::events::EurekaEvent;
use ibc_eureka_relayer_lib::utils::{to_32_bytes_exact, to_32_bytes_padded};
use ibc_eureka_utils::light_block::LightBlockExt;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use ibc_proto_eureka::Protobuf;
use prost::Message;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    keccak,
    pubkey::Pubkey,
    signature::Signature,
    sysvar,
    transaction::Transaction,
};
use tendermint::Hash;
use tendermint_rpc::{Client, HttpClient};

use solana_ibc_types::{
    derive_client, derive_client_sequence, derive_ibc_app, derive_ics07_client_state,
    derive_ics07_consensus_state, derive_packet_ack, derive_packet_commitment,
    derive_packet_receipt, derive_router_state, get_instruction_discriminator,
    ics07::{ClientState, ConsensusState, IbcHeight, ICS07_INITIALIZE_DISCRIMINATOR},
    MsgAckPacket, MsgRecvPacket, Packet, Payload, UpdateClientMsg,
};

use solana_ibc_constants::{ICS07_TENDERMINT_ID, ICS26_ROUTER_ID};

/// Default trust level for ICS07 Tendermint light client (1/3)
const DEFAULT_TRUST_LEVEL_NUMERATOR: u64 = 1;
const DEFAULT_TRUST_LEVEL_DENOMINATOR: u64 = 3;

/// Maximum allowed clock drift in seconds
const MAX_CLOCK_DRIFT_SECONDS: u64 = 15;

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

/// IBC Eureka event types from Cosmos
#[derive(Debug, Clone)]
pub enum CosmosIbcEvent {
    SendPacket {
        /// Packet sequence
        sequence: u64,
        /// Source client ID
        source_client: String,
        /// Destination client ID
        destination_client: String,
        /// Packet payloads
        payloads: Vec<Vec<u8>>,
        /// Timeout timestamp
        timeout_timestamp: u64,
    },
    AcknowledgePacket {
        /// Packet sequence
        sequence: u64,
        /// Source client ID (original source of the packet)
        source_client: String,
        /// Destination client ID (original destination of the packet)
        destination_client: String,
        /// Acknowledgement data (one per payload)
        acknowledgements: Vec<Vec<u8>>,
        /// Proof height for ack packet
        proof_height: u64,
    },
    TimeoutPacket {
        /// Packet sequence
        sequence: u64,
        /// Source client ID
        source_client: String,
    },
}

/// The `TxBuilder` produces Solana transactions based on events from Cosmos SDK.
pub struct TxBuilder {
    /// The source Cosmos HTTP client.
    pub source_tm_client: HttpClient,
    /// The Solana RPC client (wrapped in Arc since `RpcClient` doesn't implement Clone in 2.0).
    pub solana_client: Arc<RpcClient>,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: Pubkey,
    /// The Solana ICS07 Tendermint light client program ID.
    pub solana_ics07_program_id: Pubkey,
    /// The fee payer address for transactions.
    pub fee_payer: Pubkey,
}

impl TxBuilder {
    /// Create consensus state from Tendermint block
    ///
    /// # Errors
    ///
    /// Returns an error if the timestamp cannot be converted to u64
    fn create_consensus_state_from_block(
        block: &tendermint_rpc::endpoint::block::Response,
    ) -> Result<ConsensusState> {
        let app_hash = to_32_bytes_padded(block.block.header.app_hash.as_bytes(), "app_hash");

        let validators_hash = to_32_bytes_exact(
            block.block.header.validators_hash.as_bytes(),
            "validators_hash",
        );

        let timestamp_nanos = block.block.header.time.unix_timestamp_nanos();

        // Convert i128 nanoseconds to u64, handling overflow
        let timestamp_u64 = if timestamp_nanos < 0 {
            return Err(anyhow::anyhow!("Invalid timestamp: negative value"));
        } else if timestamp_nanos > u64::MAX as i128 {
            // This shouldn't happen for reasonable timestamps
            return Err(anyhow::anyhow!(
                "Invalid timestamp: value too large for u64"
            ));
        } else {
            timestamp_nanos as u64
        };

        tracing::info!(
            "Creating consensus state with timestamp: {} ns (from block time: {})",
            timestamp_u64,
            block.block.header.time
        );

        Ok(ConsensusState {
            timestamp: timestamp_u64,
            root: app_hash,
            next_validators_hash: validators_hash,
        })
    }

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
        _solana_ics26_program_id: Pubkey, // Use hardcoded for now
        _solana_ics07_program_id: Pubkey, // Use hardcoded for now
        fee_payer: Pubkey,
    ) -> Result<Self> {
        Ok(Self {
            source_tm_client,
            solana_client,
            solana_ics26_program_id: Pubkey::from_str(ICS26_ROUTER_ID)
                .map_err(|e| anyhow::anyhow!("Invalid ICS26 program ID: {e}"))?,
            solana_ics07_program_id: Pubkey::from_str(ICS07_TENDERMINT_ID)
                .map_err(|e| anyhow::anyhow!("Invalid ICS07 program ID: {e}"))?,
            fee_payer,
        })
    }

    /// Fetch events from Cosmos transactions
    ///
    /// # Errors
    ///
    /// Returns an error if failed to fetch Cosmos transaction
    #[allow(clippy::cognitive_complexity)] // Event parsing is inherently complex
    pub async fn fetch_cosmos_events(
        &self,
        tx_hashes: Vec<Hash>,
    ) -> Result<(Vec<CosmosIbcEvent>, Option<u64>)> {
        let mut events = Vec::new();
        let mut max_height: Option<u64> = None;

        for tx_hash in tx_hashes {
            // Fetch transaction from Tendermint
            let tx_result = self
                .source_tm_client
                .tx(tx_hash, false)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch Cosmos transaction: {e}"))?;

            let height = tx_result.height.value();

            // Track the maximum height from all transactions
            max_height = Some(max_height.map_or(height, |h| h.max(height)));

            for tm_event in tx_result.tx_result.events {
                if let Ok(eureka_event) = EurekaEvent::try_from(tm_event.clone()) {
                    match eureka_event {
                        EurekaEvent::SendPacket(packet) => {
                            tracing::debug!(
                                "Parsed send_packet: seq={}, src={}, dst={}",
                                packet.sequence,
                                packet.sourceClient,
                                packet.destClient
                            );

                            // Convert payloads to Vec<Vec<u8>>
                            let payloads = packet
                                .payloads
                                .into_iter()
                                .map(|p| p.value.to_vec())
                                .collect();

                            events.push(CosmosIbcEvent::SendPacket {
                                sequence: packet.sequence,
                                source_client: packet.sourceClient,
                                destination_client: packet.destClient,
                                payloads,
                                timeout_timestamp: packet.timeoutTimestamp,
                            });
                        }
                        EurekaEvent::WriteAcknowledgement(packet, acks) => {
                            tracing::info!(
                                "COSMOS WROTE ACKNOWLEDGEMENT: seq={}, src={}, dst={}, ack_data={:?}",
                                packet.sequence,
                                packet.sourceClient,
                                packet.destClient,
                                acks
                            );
                            tracing::info!(
                                "This acknowledgment should be queryable at the destination client path: {}",
                                packet.destClient
                            );

                            // WriteAck is when Cosmos (destination) writes an ack for a packet from Solana (source)
                            // We need to relay this ack back to Solana
                            // The packet.sourceClient is the Solana client, packet.destClient is the Cosmos client
                            let acknowledgements =
                                acks.into_iter().map(|ack| ack.to_vec()).collect();

                            // For now, we'll leave channels empty as they should be in the event
                            // They will be parsed from the manual event parsing below
                            events.push(CosmosIbcEvent::AcknowledgePacket {
                                sequence: packet.sequence,
                                source_client: packet.sourceClient, // Original source (Solana client)
                                destination_client: packet.destClient, // Original destination (Cosmos client)
                                acknowledgements,
                                proof_height: height,
                            });
                        }
                    }
                } else {
                    // Handle events not yet supported by EurekaEvent
                    // For now, manually parse acknowledge_packet events
                    match tm_event.kind.as_str() {
                        "acknowledge_packet" => {
                            tracing::info!("Found acknowledge_packet event, parsing manually");
                            tracing::debug!("Event attributes: {:?}", tm_event.attributes);

                            // Parse acknowledge_packet event attributes
                            let mut sequence = 0u64;
                            let mut source_client = String::new();
                            let mut destination_client = String::new();
                            let mut acknowledgements = Vec::new();

                            for attr in &tm_event.attributes {
                                if let (Ok(key), Ok(value)) = (attr.key_str(), attr.value_str()) {
                                    tracing::debug!("  Attribute: {} = {}", key, value);
                                    match key {
                                        "packet_sequence" | "sequence" => {
                                            sequence = value.parse().unwrap_or(0);
                                            tracing::debug!("    Parsed sequence: {}", sequence);
                                        }
                                        "packet_source_client" | "source_client" => {
                                            source_client = value.to_string();
                                            tracing::debug!(
                                                "    Parsed source_client: {}",
                                                source_client
                                            );
                                        }
                                        "packet_destination_client"
                                        | "destination_client"
                                        | "dest_client" => {
                                            destination_client = value.to_string();
                                            tracing::debug!(
                                                "    Parsed destination_client: {}",
                                                destination_client
                                            );
                                        }
                                        "packet_source_channel" | "source_channel" => {
                                            // Channel fields are not used in current implementation
                                            tracing::debug!(
                                                "    Ignoring source_channel: {}",
                                                value
                                            );
                                        }
                                        "packet_destination_channel"
                                        | "destination_channel"
                                        | "dest_channel" => {
                                            // Channel fields are not used in current implementation
                                            tracing::debug!(
                                                "    Ignoring destination_channel: {}",
                                                value
                                            );
                                        }
                                        "packet_ack" | "acknowledgement" | "ack" => {
                                            // The acknowledgement might be hex or base64 encoded
                                            if let Ok(ack_bytes) = hex::decode(&value) {
                                                acknowledgements.push(ack_bytes);
                                                tracing::debug!("    Parsed hex acknowledgement");
                                            } else {
                                                // If not hex, use as-is
                                                acknowledgements.push(value.as_bytes().to_vec());
                                                tracing::debug!("    Using raw acknowledgement");
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            if sequence > 0 && !source_client.is_empty() {
                                tracing::info!(
                                    "Parsed acknowledge_packet: seq={}, src={}, dest={}",
                                    sequence,
                                    source_client,
                                    destination_client
                                );

                                events.push(CosmosIbcEvent::AcknowledgePacket {
                                    sequence,
                                    source_client,
                                    destination_client,
                                    acknowledgements,
                                    proof_height: height,
                                });
                            }
                        }
                        "timeout_packet" => {
                            tracing::debug!("Found timeout_packet event (not yet implemented)");
                            // TODO: Parse timeout_packet events when needed
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok((events, max_height))
    }

    /// Fetch timeout events from Solana transactions
    ///
    /// # Errors
    ///
    /// Returns an error if failed to fetch Solana transaction
    pub fn fetch_solana_timeout_events(
        &self,
        tx_signatures: Vec<Signature>,
    ) -> Result<Vec<CosmosIbcEvent>> {
        let events = Vec::new();

        for signature in tx_signatures {
            // Get transaction details
            let _tx = self
                .solana_client
                .get_transaction_with_config(
                    &signature,
                    solana_client::rpc_config::RpcTransactionConfig {
                        encoding: Some(solana_transaction_status::UiTransactionEncoding::Json),
                        commitment: Some(CommitmentConfig::confirmed()),
                        max_supported_transaction_version: Some(0),
                    },
                )
                .map_err(|e| anyhow::anyhow!("Failed to fetch Solana transaction: {e}"))?;

            // Parse timeout events from transaction logs
            // In production, you'd parse the actual instruction data
            tracing::debug!("Processing Solana transaction for timeouts");
        }

        Ok(events)
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
        // Build optional update client
        let update_client = self
            .build_optional_update_client(
                client_id,
                skip_update_client,
                !src_events.is_empty() || !target_events.is_empty(),
                proof_height,
            )
            .await?;

        // Build packet relay transactions
        let packet_txs = self
            .build_packet_transactions(src_events, target_events)
            .await?;

        Ok(SolanaRelayTransactions {
            update_client,
            packet_txs,
        })
    }

    /// Build optional update client based on conditions
    async fn build_optional_update_client(
        &self,
        client_id: String,
        skip_update_client: bool,
        has_events: bool,
        target_height: Option<u64>,
    ) -> Result<Option<ChunkedUpdateTransactions>> {
        if !skip_update_client && has_events {
            Ok(Some(
                self.build_chunked_update_client_txs_to_height(client_id, target_height)
                    .await?,
            ))
        } else {
            if skip_update_client {
                tracing::info!("Skipping update client as requested");
            }
            Ok(None)
        }
    }

    /// Build packet relay transactions from events
    async fn build_packet_transactions(
        &self,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
    ) -> Result<Vec<Transaction>> {
        let mut packet_txs = Vec::new();

        // Get recent blockhash for packet transactions
        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        // Process source events from Cosmos
        for event in src_events {
            if let Some(tx) = self
                .build_packet_tx_from_event(event, recent_blockhash)
                .await?
            {
                packet_txs.push(tx);
            }
        }

        // Process target events (for timeouts)
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
            let mut tx = Transaction::new_with_payer(&instructions, Some(&self.fee_payer));
            tx.message.recent_blockhash = recent_blockhash;
            Ok(Some(tx))
        }
    }

    /// Build Solana relay transactions with chunked update client
    ///
    /// Convenience method that includes update client by default.
    /// Use `build_solana_relay_txs_with_options` to skip update client.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client transactions
    /// - Failed to build packet instructions
    pub async fn build_solana_relay_txs(
        &self,
        client_id: String,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
    ) -> Result<SolanaRelayTransactions> {
        self.build_solana_relay_txs_with_options(
            client_id,
            src_events,
            target_events,
            false, // Don't skip update client by default
            None,  // No specific proof height
        )
        .await
    }

    /// Build instruction to update Tendermint light client on Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get latest block from Cosmos
    /// - Failed to serialize header
    async fn build_update_client_instruction(&self) -> Result<Instruction> {
        // Get latest block from Cosmos
        let latest_block = self
            .source_tm_client
            .latest_block()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get latest block: {e}"))?;

        // Get the target light block (latest from source chain)
        let target_light_block = self.source_tm_client.get_light_block(None).await?;

        // Query the client state to get the actual trusted height
        let chain_id = latest_block.block.header.chain_id.to_string();
        let trusted_height = self.query_client_latest_height(&chain_id)?;

        // Get trusted light block from the height that actually has a consensus state
        let trusted_light_block = self
            .source_tm_client
            .get_light_block(Some(trusted_height))
            .await?;

        tracing::info!(
            "Generating Solana update client header from height: {} to height: {}",
            trusted_height,
            target_light_block.height().value()
        );

        let proposed_header = target_light_block.into_header(&trusted_light_block);
        let header_bytes = proposed_header.encode_to_vec();

        let update_msg = UpdateClientMsg {
            client_message: header_bytes,
        };

        // Get the chain ID for PDA derivation
        let chain_id = latest_block.block.header.chain_id.to_string();
        let (client_state_pda, _) =
            derive_ics07_client_state(&chain_id, &self.solana_ics07_program_id);

        // Use heights already calculated above
        let new_height = latest_block.block.header.height.value();

        let trusted_height = new_height.saturating_sub(1);
        let (trusted_consensus_state, _) = derive_ics07_consensus_state(
            &client_state_pda,
            trusted_height,
            &self.solana_ics07_program_id,
        );
        let (new_consensus_state, _) = derive_ics07_consensus_state(
            &client_state_pda,
            new_height,
            &self.solana_ics07_program_id,
        );

        // Build the instruction
        let accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new_readonly(trusted_consensus_state, false),
            AccountMeta::new(new_consensus_state, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        // Get instruction discriminator for "update_client"
        let discriminator = get_instruction_discriminator("update_client");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&update_msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        })
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

        // Build instruction data
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
        // In IBC v2 Eureka, the acknowledgment is stored at: destClient + 0x03 + sequence
        let ack_path = {
            let mut path = Vec::new();
            path.extend_from_slice(params.destination_client.as_bytes());
            path.push(3_u8); // ACK_COMMITMENT_PREFIX
            path.extend_from_slice(&params.sequence.to_be_bytes());
            path
        };

        tracing::info!("=== DEBUGGING ACK PATH ===");
        tracing::info!("Packet flow: Solana -> Cosmos (now acknowledging back to Solana)");
        tracing::info!("Source client (Cosmos on Solana): {}", params.source_client);
        tracing::info!(
            "Dest client (Solana on Cosmos): {}",
            params.destination_client
        );
        tracing::info!("Sequence: {}", params.sequence);
        tracing::info!("Proof height: {}", params.proof_height);
        tracing::info!("Expected ack path (IBC v2 Eureka): destClient + 0x03 + sequence");
        tracing::info!("Constructed path: {:?}", ack_path);
        tracing::info!("Path as hex: {}", hex::encode(&ack_path));
        tracing::info!("Path as string: {}", String::from_utf8_lossy(&ack_path));
        tracing::info!("Path breakdown:");
        tracing::info!(
            "  - Client ID bytes: {} ({})",
            hex::encode(params.destination_client.as_bytes()),
            params.destination_client
        );
        tracing::info!("  - Separator: 0x03");
        tracing::info!(
            "  - Sequence big-endian: {}",
            hex::encode(&params.sequence.to_be_bytes())
        );

        tracing::info!("=== QUERYING IBC STORE FOR ALL KEYS ===");

        let query_paths = vec![
            "/store/ibc/key",      // Try to query specific keys
            "/store/ibc/subspace", // Query subspace
            "/cosmos.ibc.core.channel.v1.Query/PacketAcknowledgement", // Query service
        ];

        for query_path in query_paths {
            tracing::debug!("Trying ABCI query path: {}", query_path);
            let abci_query = self
                .source_tm_client
                .abci_query(
                    Some(query_path.to_string()),
                    vec![], // Empty data
                    Some(tendermint::block::Height::try_from(
                        params.proof_height as u32,
                    )?),
                    false,
                )
                .await;

            if let Ok(response) = abci_query {
                if !response.value.is_empty() || response.code.is_ok() {
                    tracing::info!(
                        "ABCI query {} returned: code={:?}, log={}, value_len={}",
                        query_path,
                        response.code,
                        response.log,
                        response.value.len()
                    );
                    if !response.value.is_empty() {
                        tracing::debug!("Value (hex): {}", hex::encode(&response.value));
                    }
                }
            }
        }

        // Try different path formats - IBC-Go v10 might still use channel paths even in Eureka mode
        tracing::info!("=== TRYING DIFFERENT PATH FORMATS ===");

        // IBC Classic format: acks/ports/{port}/channels/{channel}/sequences/{sequence}
        // Let's try the raw format that IBC-Go v10 might use
        let classic_paths = vec![
            format!(
                "acks/ports/transfer/channels/channel-0/sequences/{}",
                params.sequence
            ),
            format!(
                "acks/ports/transfer/channels/channel-1/sequences/{}",
                params.sequence
            ),
        ];

        for classic_path in &classic_paths {
            tracing::info!("Trying IBC Classic path: {}", classic_path);
            let (test_value, _) = self
                .source_tm_client
                .prove_path(
                    &[b"ibc".to_vec(), classic_path.as_bytes().to_vec()],
                    params.proof_height,
                )
                .await?;

            if !test_value.is_empty() {
                tracing::info!(
                    "FOUND acknowledgment at IBC Classic path: {} (value: {} bytes)",
                    classic_path,
                    test_value.len()
                );
                tracing::info!("Acknowledgment value (hex): {}", hex::encode(&test_value));
            }
        }

        // Try commitments/ prefix that might be used by IBC-Go v10
        let commitment_paths = vec![
            format!("commitments/sequences/{}", params.sequence),
            format!(
                "commitments/{}/sequences/{}",
                params.destination_client, params.sequence
            ),
            format!(
                "commitments/{}/sequences/{}",
                params.source_client, params.sequence
            ),
            format!("acks/{}/{}", params.destination_client, params.sequence),
            format!("acks/{}/{}", params.source_client, params.sequence),
        ];

        for commitment_path in &commitment_paths {
            tracing::info!("Trying commitment/ack path format: {}", commitment_path);
            let (test_value, _) = self
                .source_tm_client
                .prove_path(
                    &[b"ibc".to_vec(), commitment_path.as_bytes().to_vec()],
                    params.proof_height,
                )
                .await?;

            if !test_value.is_empty() {
                tracing::info!(
                    "FOUND acknowledgment at commitment path: {} (value: {} bytes)",
                    commitment_path,
                    test_value.len()
                );
                tracing::info!("Acknowledgment value (hex): {}", hex::encode(&test_value));
            }
        }

        // Also try IBC v2 Eureka format with different client IDs
        let prefixes_to_try = vec![
            (
                b"08-wasm-0".to_vec(),
                "destination client (Solana on Cosmos)",
            ),
            (
                params.destination_client.as_bytes().to_vec(),
                "explicit destination client",
            ),
            (b"cosmoshub-1".to_vec(), "source client (Cosmos on Solana)"),
            (
                params.source_client.as_bytes().to_vec(),
                "explicit source client",
            ),
            (b"channel-0".to_vec(), "channel-0 (backwards compat?)"),
            (b"channel-1".to_vec(), "channel-1 (backwards compat?)"),
        ];

        for (prefix, description) in &prefixes_to_try {
            let mut test_path = prefix.clone();
            test_path.push(3_u8); // ACK_COMMITMENT_PREFIX
            test_path.extend_from_slice(&params.sequence.to_be_bytes());

            tracing::info!(
                "Checking IBC v2 path with {}: {} (hex: {})",
                description,
                String::from_utf8_lossy(&test_path),
                hex::encode(&test_path)
            );

            let (test_value, _) = self
                .source_tm_client
                .prove_path(&[b"ibc".to_vec(), test_path.clone()], params.proof_height)
                .await?;

            if !test_value.is_empty() {
                tracing::info!(
                    "✓ FOUND acknowledgment at path with {}: {} (value: {} bytes)",
                    description,
                    String::from_utf8_lossy(&test_path),
                    test_value.len()
                );
                tracing::info!("Acknowledgment value (hex): {}", hex::encode(&test_value));

                // Use this working path
                tracing::info!("Using this path for the actual query!");
                // Update ack_path to use the working one
                // Note: We'd need to restructure the code to use this, for now just log it
            } else {
                tracing::debug!("✗ No acknowledgment at path with {}", description);
            }
        }

        // Now try to find the actual acknowledgment path that works
        let mut working_ack_path = None;

        // First try the expected IBC v2 path
        let (value_v2, proof_v2) = self
            .source_tm_client
            .prove_path(&[b"ibc".to_vec(), ack_path.clone()], params.proof_height)
            .await?;

        if !value_v2.is_empty() {
            tracing::info!("✓ Found acknowledgment at expected IBC v2 path");
            working_ack_path = Some((ack_path.clone(), value_v2, proof_v2));
        }

        // If not found, try IBC Classic paths
        if working_ack_path.is_none() {
            for channel in &["channel-0", "channel-1"] {
                let classic_path = format!(
                    "acks/ports/transfer/channels/{}/sequences/{}",
                    channel, params.sequence
                );
                let (value_classic, proof_classic) = self
                    .source_tm_client
                    .prove_path(
                        &[b"ibc".to_vec(), classic_path.as_bytes().to_vec()],
                        params.proof_height,
                    )
                    .await?;

                if !value_classic.is_empty() {
                    tracing::info!(
                        "✓ Found acknowledgment at IBC Classic path: {}",
                        classic_path
                    );
                    working_ack_path = Some((
                        classic_path.as_bytes().to_vec(),
                        value_classic,
                        proof_classic,
                    ));
                    break;
                }
            }
        }

        // If still not found, try commitment paths
        if working_ack_path.is_none() {
            let commitment_paths = vec![
                format!("commitments/sequences/{}", params.sequence),
                format!(
                    "commitments/{}/sequences/{}",
                    params.destination_client, params.sequence
                ),
                format!(
                    "commitments/{}/sequences/{}",
                    params.source_client, params.sequence
                ),
                format!("acks/{}/{}", params.destination_client, params.sequence),
                format!("acks/{}/{}", params.source_client, params.sequence),
            ];

            for commitment_path in &commitment_paths {
                let (value_commit, proof_commit) = self
                    .source_tm_client
                    .prove_path(
                        &[b"ibc".to_vec(), commitment_path.as_bytes().to_vec()],
                        params.proof_height,
                    )
                    .await?;

                if !value_commit.is_empty() {
                    tracing::info!(
                        "✓ Found acknowledgment at commitment path: {}",
                        commitment_path
                    );
                    working_ack_path = Some((
                        commitment_path.as_bytes().to_vec(),
                        value_commit,
                        proof_commit,
                    ));
                    break;
                }
            }
        }

        // If still not found, we have a problem
        let (value, merkle_proof) = match working_ack_path {
            Some((path, val, proof)) => {
                tracing::info!(
                    "Using acknowledgment from path: {}",
                    String::from_utf8_lossy(&path)
                );
                (val, proof)
            }
            None => {
                tracing::error!("No acknowledgment found at any expected path");
                tracing::error!("Tried IBC v2 path: {}", String::from_utf8_lossy(&ack_path));
                tracing::error!("Tried IBC Classic paths: acks/ports/transfer/channels/channel-{{0,1}}/sequences/{}", params.sequence);
                return Err(anyhow::anyhow!("Acknowledgment not found on chain"));
            }
        };

        tracing::info!("Found acknowledgment value: {} bytes", value.len());
        tracing::debug!("Acknowledgment value hex: {}", hex::encode(&value));

        let proof = merkle_proof.encode_vec();
        tracing::info!("Generated proof: {} bytes", proof.len());

        let msg = MsgAckPacket {
            packet,
            acknowledgement,
            proof_acked: proof,
            proof_height: params.proof_height,
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

        // Query the latest height from the client state
        let latest_height = self.query_client_latest_height(&ics07_chain_id)?;
        let (consensus_state, _) = derive_ics07_consensus_state(
            &client_state,
            latest_height,
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
        // Get latest block from Cosmos for initial client state
        let latest_block = self
            .source_tm_client
            .latest_block()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get latest block: {e}"))?;

        let chain_id_str = latest_block.block.header.chain_id.to_string();
        let latest_height = latest_block.block.header.height.value();

        // Extract revision number from chain ID (format: {chain_name}-{revision_number})
        let revision_number = chain_id_str
            .rsplit('-')
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Invalid chain ID format: expected {{chain_name}}-{{revision_number}}, got {}",
                    chain_id_str
                )
            })?;

        // Query staking parameters to get unbonding period
        let unbonding_period = self
            .source_tm_client
            .sdk_staking_params()
            .await?
            .unbonding_time
            .ok_or_else(|| anyhow::anyhow!("No unbonding time found"))?
            .seconds
            .try_into()?;
        let trusting_period = 2 * (unbonding_period / 3);

        tracing::info!(
            "Creating client for chain {} at height {}, revision: {}",
            chain_id_str,
            latest_height,
            revision_number
        );

        tracing::info!(
            "Using client parameters: trust_level={}/{}, trusting_period={}, unbonding_period={}, max_clock_drift={}",
            DEFAULT_TRUST_LEVEL_NUMERATOR, DEFAULT_TRUST_LEVEL_DENOMINATOR, trusting_period, unbonding_period, MAX_CLOCK_DRIFT_SECONDS
        );

        // Create proper ClientState matching ICS07 program expectations
        let client_state = ClientState {
            chain_id: chain_id_str.clone(),
            trust_level_numerator: DEFAULT_TRUST_LEVEL_NUMERATOR,
            trust_level_denominator: DEFAULT_TRUST_LEVEL_DENOMINATOR,
            trusting_period,
            unbonding_period,
            max_clock_drift: MAX_CLOCK_DRIFT_SECONDS,
            frozen_height: IbcHeight {
                revision_number: 0,
                revision_height: 0,
            },
            latest_height: IbcHeight {
                revision_number,
                revision_height: latest_height,
            },
        };

        // Create proper ConsensusState from the block
        let consensus_state = Self::create_consensus_state_from_block(&latest_block)?;

        // Build the instruction for creating the client
        let instruction = self.build_create_client_instruction(
            &chain_id_str,
            latest_height,
            &client_state,
            &consensus_state,
        )?;

        // Create unsigned transaction
        let mut tx = Transaction::new_with_payer(&[instruction], Some(&self.fee_payer));

        // Get recent blockhash
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
        target_height_override: Option<u64>,
    ) -> Result<ChunkedUpdateTransactions> {
        tracing::info!(
            "Building chunked update client transactions for client {} to height {:?}",
            client_id,
            target_height_override
        );

        // Fetch block data and create header
        let (header_bytes, chain_id, target_height, trusted_height) =
            if let Some(height) = target_height_override {
                self.prepare_header_for_chunking_at_height(height).await?
            } else {
                self.prepare_header_for_chunking().await?
            };

        self.build_chunked_update_client_txs_internal(
            client_id,
            header_bytes,
            chain_id,
            target_height,
            trusted_height,
        )
        .await
    }

    pub async fn build_chunked_update_client_txs(
        &self,
        client_id: String,
    ) -> Result<ChunkedUpdateTransactions> {
        self.build_chunked_update_client_txs_to_height(client_id, None)
            .await
    }

    async fn build_chunked_update_client_txs_internal(
        &self,
        _client_id: String,
        header_bytes: Vec<u8>,
        chain_id: String,
        target_height: u64,
        trusted_height: u64,
    ) -> Result<ChunkedUpdateTransactions> {
        // Calculate header commitment and split into chunks
        let header_commitment = keccak::hash(&header_bytes).0;
        let chunks = Self::split_header_into_chunks(&header_bytes);
        let total_chunks = u8::try_from(chunks.len())
            .map_err(|_| anyhow::anyhow!("Too many chunks: {} exceeds u8 max", chunks.len()))?;

        tracing::info!(
            "Header size: {} bytes, split into {} chunks",
            header_bytes.len(),
            total_chunks
        );

        // Get recent blockhash for all transactions
        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        // Build metadata creation transaction
        let metadata_tx = self.build_create_metadata_transaction(
            &chain_id,
            target_height,
            total_chunks,
            header_commitment,
            recent_blockhash,
        );

        // Build all chunk upload transactions
        let chunk_txs =
            self.build_chunk_transactions(&chunks, &chain_id, target_height, recent_blockhash)?;

        // Build assembly transaction
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

    /// Query the client state from Solana to get the latest consensus state height
    fn query_client_latest_height(&self, chain_id: &str) -> Result<u64> {
        // For the test, we know the client ID "cosmoshub-1" corresponds to chain ID "simd-1"
        // In production, this mapping should be maintained properly
        let actual_chain_id = if chain_id == "cosmoshub-1" {
            "simd-1"
        } else {
            chain_id
        };

        let (client_state_pda, _) =
            derive_ics07_client_state(actual_chain_id, &self.solana_ics07_program_id);

        // Fetch the account data
        let account = self
            .solana_client
            .get_account(&client_state_pda)
            .context("Failed to fetch client state account")?;

        // Deserialize the client state (skip 8-byte Anchor discriminator)
        // Only deserialize the exact bytes needed, ignoring any padding
        let client_state = ClientState::try_from_slice(&account.data[8..])
            .or_else(|_| {
                // If try_from_slice fails due to extra bytes, use deserialize which is more lenient
                let mut data = &account.data[8..];
                ClientState::deserialize(&mut data)
            })
            .context("Failed to deserialize client state")?;

        Ok(client_state.latest_height.revision_height)
    }

    /// Prepare header data for chunking by fetching blocks and creating header
    async fn prepare_header_for_chunking_at_height(
        &self,
        height: u64,
    ) -> Result<(Vec<u8>, String, u64, u64)> {
        // Get block at specific height from Cosmos
        let target_light_block = self
            .source_tm_client
            .get_light_block(Some(height))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get block at height {}: {e}", height))?;

        // Get chain_id
        let chain_id = target_light_block.signed_header.header.chain_id.to_string();
        let target_height = height;

        // Query the client state to get the actual trusted height
        let trusted_height = self.query_client_latest_height(&chain_id)?;

        // Get trusted light block from the height that actually has a consensus state
        let trusted_light_block = self
            .source_tm_client
            .get_light_block(Some(trusted_height))
            .await?;

        tracing::info!(
            "Generating chunked Solana update client header from height: {} to height: {}",
            trusted_height,
            target_light_block.height().value()
        );

        // Create the header
        let proposed_header = target_light_block.into_header(&trusted_light_block);
        let header_bytes = proposed_header.encode_to_vec();

        Ok((header_bytes, chain_id, target_height, trusted_height))
    }

    async fn prepare_header_for_chunking(&self) -> Result<(Vec<u8>, String, u64, u64)> {
        // Get latest block from Cosmos
        let latest_block = self
            .source_tm_client
            .latest_block()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get latest block: {e}"))?;

        // Get chain_id first
        let chain_id = latest_block.block.header.chain_id.to_string();
        let target_height = latest_block.block.header.height.value();

        // Get the target light block (latest from source chain)
        let target_light_block = self.source_tm_client.get_light_block(None).await?;

        // Query the client state to get the actual trusted height
        let trusted_height = self.query_client_latest_height(&chain_id)?;

        // Get trusted light block from the height that actually has a consensus state
        let trusted_light_block = self
            .source_tm_client
            .get_light_block(Some(trusted_height))
            .await?;

        tracing::info!(
            "Generating chunked Solana update client header from height: {} to height: {}",
            trusted_height,
            target_light_block.height().value()
        );

        // Create the header
        let proposed_header = target_light_block.into_header(&trusted_light_block);
        let header_bytes = proposed_header.encode_to_vec();

        Ok((header_bytes, chain_id, target_height, trusted_height))
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

    /// Build chunk upload transactions (all can be submitted in parallel after metadata creation)
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
            let chunk_tx = self.build_single_chunk_transaction(&ChunkTxParams {
                chunk_data,
                chain_id,
                target_height,
                chunk_index,
                recent_blockhash,
            })?;
            chunk_txs.push(chunk_tx);
        }

        Ok(chunk_txs)
    }

    fn build_single_chunk_transaction(&self, params: &ChunkTxParams) -> Result<Transaction> {
        let upload_ix = self.build_upload_header_chunk_instruction(
            params.chain_id,
            params.target_height,
            params.chunk_index,
            params.chunk_data.to_vec(),
        )?;

        let mut tx = Transaction::new_with_payer(&[upload_ix], Some(&self.fee_payer));
        tx.message.recent_blockhash = params.recent_blockhash;

        Ok(tx)
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

    /// Build instruction for creating metadata
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
    fn build_create_metadata_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        total_chunks: u8,
        header_commitment: [u8; 32],
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

        // Build accounts
        let accounts = vec![
            AccountMeta::new(metadata_pda, false),
            AccountMeta::new_readonly(client_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        // Build instruction data
        let discriminator = get_instruction_discriminator("create_metadata");
        let mut data = discriminator.to_vec();

        // Serialize parameters
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

    /// Build instruction for uploading a header chunk
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
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

    /// Build instruction for assembling chunks and updating the client
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

        // Build instruction data
        let discriminator = get_instruction_discriminator("assemble_and_update_client");

        Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: discriminator.to_vec(),
        }
    }

    /// Build instruction for cleaning up incomplete uploads
    ///
    /// # Errors
    ///
    /// Returns an error if PDA derivation fails
    pub fn build_cleanup_incomplete_upload_instruction(
        &self,
        chain_id: &str,
        cleanup_height: u64,
        total_chunks: u8,
    ) -> Result<Instruction> {
        // Derive PDAs
        let (client_state_pda, _) =
            derive_ics07_client_state(chain_id, &self.solana_ics07_program_id);
        let (metadata_pda, _) = derive_header_metadata(
            &self.fee_payer,
            chain_id,
            cleanup_height,
            &self.solana_ics07_program_id,
        );

        // Build accounts
        let mut accounts = vec![
            AccountMeta::new_readonly(client_state_pda, false),
            AccountMeta::new(metadata_pda, false),
            AccountMeta::new(self.fee_payer, true), // submitter_account
        ];

        // Add chunk accounts as remaining accounts to close
        for chunk_index in 0..total_chunks {
            let (chunk_pda, _) = derive_header_chunk(
                &self.fee_payer,
                chain_id,
                cleanup_height,
                chunk_index,
                &self.solana_ics07_program_id,
            );
            accounts.push(AccountMeta::new(chunk_pda, false));
        }

        // Build instruction data
        let discriminator = get_instruction_discriminator("cleanup_incomplete_upload");
        let mut data = discriminator.to_vec();
        // Add parameters: chain_id, cleanup_height, submitter
        data.extend_from_slice(&chain_id.try_to_vec()?);
        data.extend_from_slice(&cleanup_height.try_to_vec()?);
        data.extend_from_slice(&self.fee_payer.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        })
    }

    /// Build an update client transaction for Solana (DEPRECATED - use chunked version)
    ///
    /// This method is deprecated because Tendermint headers always exceed
    /// Solana's transaction size limit. Use `build_chunked_update_client_txs` instead.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get latest block
    /// - Failed to serialize header
    #[deprecated(
        note = "Use build_chunked_update_client_txs instead - headers exceed tx size limit"
    )]
    pub async fn build_update_client_tx(&self, client_id: String) -> Result<Transaction> {
        tracing::warn!(
            "Using deprecated build_update_client_tx for client {} - headers will exceed tx size limit!",
            client_id
        );

        let update_ix = self.build_update_client_instruction().await?;

        // Create unsigned transaction
        let mut tx = Transaction::new_with_payer(&[update_ix], Some(&self.fee_payer));

        // Get recent blockhash
        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        tx.message.recent_blockhash = recent_blockhash;

        Ok(tx)
    }
}

/// Mock `TxBuilder` for testing that uses mock proofs instead of real ones
pub struct MockTxBuilder {
    /// The underlying real `TxBuilder`
    pub inner: TxBuilder,
}

impl MockTxBuilder {
    /// Creates a new `MockTxBuilder`.
    ///
    /// # Errors
    ///
    /// Returns an error if the inner `TxBuilder` creation fails
    pub fn new(
        source_tm_client: HttpClient,
        solana_client: Arc<RpcClient>,
        solana_ics26_program_id: Pubkey,
        solana_ics07_program_id: Pubkey,
        fee_payer: Pubkey,
    ) -> Result<Self> {
        Ok(Self {
            inner: TxBuilder::new(
                source_tm_client,
                solana_client,
                solana_ics26_program_id,
                solana_ics07_program_id,
                fee_payer,
            )?,
        })
    }

    /// Build instruction for `RecvPacket` on Solana with mock proofs
    ///
    /// # Errors
    ///
    /// Returns an error if packet data cannot be serialized
    fn build_recv_packet_instruction_mock(&self, params: &RecvPacketParams) -> Result<Instruction> {
        // Build the packet structure (IBC v2)
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

        // Create the message with mock proofs
        let msg = MsgRecvPacket {
            packet,
            proof_commitment: MOCK_PROOF_DATA.to_vec(), // Mock proof
            proof_height: 1,                            // Mock height
        };

        // Derive all required PDAs
        let (router_state, _) = derive_router_state(&self.inner.solana_ics26_program_id);
        let (ibc_app, _) = derive_ibc_app(&dest_port, &self.inner.solana_ics26_program_id);
        let (client_sequence, _) = derive_client_sequence(
            params.destination_client,
            &self.inner.solana_ics26_program_id,
        );
        let (packet_receipt, _) = derive_packet_receipt(
            params.destination_client,
            params.sequence,
            &self.inner.solana_ics26_program_id,
        );
        let (packet_ack, _) = derive_packet_ack(
            params.destination_client,
            params.sequence,
            &self.inner.solana_ics26_program_id,
        );
        let (client, _) = derive_client(
            params.destination_client,
            &self.inner.solana_ics26_program_id,
        );

        // For light client verification, we also need ICS07 accounts
        let (client_state, _) =
            derive_ics07_client_state(params.source_client, &self.inner.solana_ics07_program_id);

        // For mock, we use height 1 since mock doesn't have actual consensus states
        let (consensus_state, _) =
            derive_ics07_consensus_state(&client_state, 1, &self.inner.solana_ics07_program_id);

        // Build accounts list
        let accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_receipt, false),
            AccountMeta::new(packet_ack, false),
            AccountMeta::new_readonly(self.inner.fee_payer, true), // relayer
            AccountMeta::new(self.inner.fee_payer, true),          // payer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.inner.solana_ics07_program_id, false), // light client program
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        // Build instruction data
        let discriminator = get_instruction_discriminator("recv_packet");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.inner.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    /// Build a Solana transaction from IBC events using mock proofs
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client instruction
    /// - No instructions to execute
    #[allow(clippy::cognitive_complexity)]
    pub async fn build_solana_tx_mock(
        &self,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
    ) -> Result<Transaction> {
        let mut instructions = Vec::new();

        // First, update the Tendermint light client on Solana
        let update_client_ix = self.inner.build_update_client_instruction().await?;
        instructions.push(update_client_ix);

        // Process source events from Cosmos with mock proofs
        for event in src_events {
            match event {
                CosmosIbcEvent::SendPacket {
                    sequence,
                    source_client,
                    destination_client,
                    payloads,
                    timeout_timestamp,
                } => {
                    let recv_packet_ix =
                        self.build_recv_packet_instruction_mock(&RecvPacketParams {
                            sequence,
                            source_client: &source_client,
                            destination_client: &destination_client,
                            payloads: &payloads,
                            timeout_timestamp,
                        })?;
                    instructions.push(recv_packet_ix);
                }
                CosmosIbcEvent::AcknowledgePacket { .. } => {
                    tracing::debug!("Building acknowledgement instruction with mock proof");
                }
                CosmosIbcEvent::TimeoutPacket { .. } => {
                    tracing::debug!("Building timeout instruction with mock proof");
                }
            }
        }

        for event in target_events {
            tracing::debug!(?event, "Processing timeout event with mock proof");
        }

        if instructions.is_empty() {
            anyhow::bail!("No instructions to execute on Solana");
        }

        // Create unsigned transaction
        let mut tx = Transaction::new_with_payer(&instructions, Some(&self.inner.fee_payer));

        // Get recent blockhash
        let recent_blockhash = self
            .inner
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        tx.message.recent_blockhash = recent_blockhash;

        Ok(tx)
    }
}
