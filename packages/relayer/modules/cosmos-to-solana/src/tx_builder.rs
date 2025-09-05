//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::str::FromStr;
use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::Result;
use ibc_eureka_relayer_lib::events::EurekaEvent;
use ibc_eureka_relayer_lib::utils::{to_32_bytes_exact, to_32_bytes_padded};
use ibc_eureka_utils::light_block::LightBlockExt;
use ibc_eureka_utils::rpc::TendermintRpcExt;
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
    derive_ics07_consensus_state, derive_packet_ack, derive_packet_receipt, derive_router_state,
    get_instruction_discriminator,
    ics07::{ClientState, ConsensusState, IbcHeight, ICS07_INITIALIZE_DISCRIMINATOR},
    MsgRecvPacket, Packet, Payload, UpdateClientMsg,
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
const MAX_CHUNK_SIZE: usize = 900;

/// Parameters for uploading a header chunk (mirrors the Solana program's type)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
struct UploadChunkParams {
    chain_id: String,
    target_height: u64,
    chunk_index: u8,
    total_chunks: u8,
    chunk_data: Vec<u8>,
    chunk_hash: [u8; 32],
    header_commitment: [u8; 32],
}

/// Parameters for building chunk transactions
struct ChunkTxParams<'a> {
    chunk_data: &'a [u8],
    chain_id: &'a str,
    target_height: u64,
    chunk_index: u8,
    total_chunks: u8,
    header_commitment: [u8; 32],
    recent_blockhash: solana_sdk::hash::Hash,
}

/// Organized transactions for chunked update client
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ChunkedUpdateTransactions {
    /// First chunk transaction (creates metadata, must be submitted first)
    pub first_chunk_tx: Transaction,
    /// Remaining chunk transactions (can be submitted in parallel)
    pub parallel_chunk_txs: Vec<Transaction>,
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
        /// Source client ID
        source_client: String,
        /// Acknowledgement data (one per payload)
        acknowledgements: Vec<Vec<u8>>,
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

        Ok(ConsensusState {
            timestamp: block
                .block
                .header
                .time
                .unix_timestamp()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid timestamp: negative value"))?,
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
    pub async fn fetch_cosmos_events(&self, tx_hashes: Vec<Hash>) -> Result<Vec<CosmosIbcEvent>> {
        let mut events = Vec::new();

        for tx_hash in tx_hashes {
            // Fetch transaction from Tendermint
            let tx_result = self
                .source_tm_client
                .tx(tx_hash, false)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch Cosmos transaction: {e}"))?;

            let _height = tx_result.height.value();

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
                        EurekaEvent::WriteAcknowledgement(packet, _acks) => {
                            tracing::debug!(
                                "Parsed write_acknowledgement: seq={}, src={}",
                                packet.sequence,
                                packet.sourceClient
                            );

                            // For now, we'll skip WriteAck as it's not the same as AcknowledgePacket
                            // WriteAck is when the destination writes an ack,
                            // AcknowledgePacket is when source processes the ack
                        }
                    }
                } else {
                    // Handle events not yet supported by EurekaEvent
                    // For now, just log them
                    match tm_event.kind.as_str() {
                        "acknowledge_packet" => {
                            tracing::debug!("Found acknowledge_packet event (not yet implemented in EurekaEvent)");
                            // TODO: When EurekaEvent supports AcknowledgePacket, handle it
                        }
                        "timeout_packet" => {
                            tracing::debug!(
                                "Found timeout_packet event (not yet implemented in EurekaEvent)"
                            );
                            // TODO: When EurekaEvent supports TimeoutPacket, handle it
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(events)
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
    ) -> Result<SolanaRelayTransactions> {
        // Build optional update client
        let update_client = self
            .build_optional_update_client(
                client_id,
                skip_update_client,
                !src_events.is_empty() || !target_events.is_empty(),
            )
            .await?;

        // Build packet relay transactions
        let packet_txs = self.build_packet_transactions(src_events, target_events)?;

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
    ) -> Result<Option<ChunkedUpdateTransactions>> {
        if !skip_update_client && has_events {
            Ok(Some(self.build_chunked_update_client_txs(client_id).await?))
        } else {
            if skip_update_client {
                tracing::info!("Skipping update client as requested");
            }
            Ok(None)
        }
    }

    /// Build packet relay transactions from events
    fn build_packet_transactions(
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
            if let Some(tx) = self.build_packet_tx_from_event(event, recent_blockhash)? {
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
    fn build_packet_tx_from_event(
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
            CosmosIbcEvent::AcknowledgePacket { .. } => {
                tracing::debug!("Building acknowledgement instruction - not yet implemented");
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

        // Get trusted light block from previous height
        let trusted_height = latest_block.block.header.height.value().saturating_sub(1);
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
        let (consensus_state, _) =
            derive_ics07_consensus_state(&client_state, 0, &self.solana_ics07_program_id); // Use appropriate height

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
    pub async fn build_chunked_update_client_txs(
        &self,
        client_id: String,
    ) -> Result<ChunkedUpdateTransactions> {
        tracing::info!(
            "Building chunked update client transactions for client {}",
            client_id
        );

        // Fetch block data and create header
        let (header_bytes, chain_id, target_height, trusted_height) =
            self.prepare_header_for_chunking().await?;

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

        // Build all chunk transactions
        let (first_chunk_tx, parallel_chunk_txs) = self.build_chunk_transactions(
            &chunks,
            &chain_id,
            target_height,
            total_chunks,
            header_commitment,
            recent_blockhash,
        )?;

        // Build assembly transaction
        let assembly_tx = self.build_assembly_transaction(
            &chain_id,
            target_height,
            trusted_height,
            total_chunks,
            recent_blockhash,
        );

        Ok(ChunkedUpdateTransactions {
            first_chunk_tx,
            parallel_chunk_txs,
            assembly_tx,
            total_chunks: total_chunks as usize,
            target_height,
        })
    }

    /// Prepare header data for chunking by fetching blocks and creating header
    async fn prepare_header_for_chunking(&self) -> Result<(Vec<u8>, String, u64, u64)> {
        // Get latest block from Cosmos
        let latest_block = self
            .source_tm_client
            .latest_block()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get latest block: {e}"))?;

        // Get the target light block (latest from source chain)
        let target_light_block = self.source_tm_client.get_light_block(None).await?;

        // Get trusted light block from previous height
        let trusted_height = latest_block.block.header.height.value().saturating_sub(1);
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

        let chain_id = latest_block.block.header.chain_id.to_string();
        let target_height = latest_block.block.header.height.value();

        Ok((header_bytes, chain_id, target_height, trusted_height))
    }

    /// Split header into chunks of `MAX_CHUNK_SIZE`
    fn split_header_into_chunks(header_bytes: &[u8]) -> Vec<Vec<u8>> {
        header_bytes
            .chunks(MAX_CHUNK_SIZE)
            .map(<[u8]>::to_vec)
            .collect()
    }

    /// Build chunk upload transactions
    fn build_chunk_transactions(
        &self,
        chunks: &[Vec<u8>],
        chain_id: &str,
        target_height: u64,
        total_chunks: u8,
        header_commitment: [u8; 32],
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<(Transaction, Vec<Transaction>)> {
        // Build first chunk transaction separately
        let first_chunk_tx = self.build_single_chunk_transaction(&ChunkTxParams {
            chunk_data: &chunks[0],
            chain_id,
            target_height,
            chunk_index: 0,
            total_chunks,
            header_commitment,
            recent_blockhash,
        })?;

        // Build remaining chunk transactions (can be submitted in parallel)
        let mut parallel_chunk_txs = Vec::new();
        for (index, chunk_data) in chunks.iter().enumerate().skip(1) {
            let chunk_index = u8::try_from(index)
                .map_err(|_| anyhow::anyhow!("Chunk index {} exceeds u8 max", index))?;
            let chunk_tx = self.build_single_chunk_transaction(&ChunkTxParams {
                chunk_data,
                chain_id,
                target_height,
                chunk_index,
                total_chunks,
                header_commitment,
                recent_blockhash,
            })?;
            parallel_chunk_txs.push(chunk_tx);
        }

        Ok((first_chunk_tx, parallel_chunk_txs))
    }

    /// Build a single chunk upload transaction
    fn build_single_chunk_transaction(&self, params: &ChunkTxParams) -> Result<Transaction> {
        let chunk_hash = keccak::hash(params.chunk_data).0;

        let upload_ix = self.build_upload_header_chunk_instruction(
            params.chain_id,
            params.target_height,
            params.chunk_index,
            params.total_chunks,
            params.chunk_data.to_vec(),
            chunk_hash,
            params.header_commitment,
        )?;

        let mut tx = Transaction::new_with_payer(&[upload_ix], Some(&self.fee_payer));
        tx.message.recent_blockhash = params.recent_blockhash;

        Ok(tx)
    }

    /// Build the assembly transaction
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

        let mut assembly_tx =
            Transaction::new_with_payer(&[assembly_instruction], Some(&self.fee_payer));
        assembly_tx.message.recent_blockhash = recent_blockhash;

        assembly_tx
    }

    /// Build instruction for uploading a header chunk
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
    #[allow(clippy::too_many_arguments)]
    fn build_upload_header_chunk_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        chunk_index: u8,
        total_chunks: u8,
        chunk_data: Vec<u8>,
        chunk_hash: [u8; 32],
        header_commitment: [u8; 32],
    ) -> Result<Instruction> {
        // Create upload chunk params
        let params = UploadChunkParams {
            chain_id: chain_id.to_string(),
            target_height,
            chunk_index,
            total_chunks,
            chunk_data,
            chunk_hash,
            header_commitment,
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
        let (metadata_pda, _) = derive_header_metadata(
            &self.fee_payer,
            chain_id,
            target_height,
            &self.solana_ics07_program_id,
        );

        // Build accounts
        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(metadata_pda, false),
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
        let (consensus_state, _) =
            derive_ics07_consensus_state(&client_state, 0, &self.inner.solana_ics07_program_id);

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
