//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::sync::Arc;

use anchor_lang::{AnchorDeserialize, AnchorSerialize};
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
    pubkey::Pubkey,
    signature::Signature,
    sysvar,
    transaction::Transaction,
};
use tendermint::Hash;
use tendermint_rpc::{Client, HttpClient};

use solana_ibc_types::{
    derive_app_state, derive_client, derive_client_sequence, derive_ibc_app,
    derive_ics07_client_state, derive_ics07_consensus_state, derive_packet_ack,
    derive_packet_receipt, derive_router_state, get_instruction_discriminator,
    ics07::{ClientState, ConsensusState, IbcHeight, ICS07_INITIALIZE_DISCRIMINATOR},
    IBCApp, MsgRecvPacket, Packet, Payload, UpdateClientMsg,
};

/// Default trust level for ICS07 Tendermint light client (1/3)
const DEFAULT_TRUST_LEVEL_NUMERATOR: u64 = 1;
const DEFAULT_TRUST_LEVEL_DENOMINATOR: u64 = 3;

/// Maximum allowed clock drift in seconds
const MAX_CLOCK_DRIFT_SECONDS: u64 = 15;

/// Mock proof data for testing purposes
const MOCK_PROOF_DATA: &[u8] = b"mock";

/// Parameters for building a `RecvPacket` instruction
pub struct RecvPacketParams<'a> {
    sequence: u64,
    source_client: &'a str,
    destination_client: &'a str,
    payloads: &'a [Payload],
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
        payloads: Vec<Payload>,
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
    #[allow(clippy::cognitive_complexity)]
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

        tracing::debug!("Program ID: {}", self.solana_ics07_program_id);
        tracing::debug!("Chain ID: {}", chain_id);
        tracing::debug!("Latest height: {}", latest_height);
        tracing::debug!("Client state PDA: {}", client_state_pda);
        tracing::debug!("Consensus state PDA: {}", consensus_state_pda);
        tracing::debug!("Fee payer: {}", self.fee_payer);

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
        tracing::debug!("Discriminator: {:?}", discriminator);

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
    pub const fn new(
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
            solana_ics07_program_id,
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
        tracing::debug!(
            "Fetching Cosmos events from {} transactions",
            tx_hashes.len()
        );

        for (i, tx_hash) in tx_hashes.iter().enumerate() {
            tracing::debug!("Fetching Cosmos transaction {}: {:02x?}", i, tx_hash);

            // Fetch transaction from Tendermint
            let tx_result = self
                .source_tm_client
                .tx(*tx_hash, false)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Failed to fetch Cosmos transaction {:02x?}: {e}", tx_hash)
                })?;

            let height = tx_result.height.value();
            tracing::debug!(
                "Successfully fetched Cosmos transaction {:02x?} at height {}",
                tx_hash,
                height
            );
            tracing::debug!(
                "Transaction has {} events",
                tx_result.tx_result.events.len()
            );

            for (event_idx, tm_event) in tx_result.tx_result.events.iter().enumerate() {
                tracing::debug!("Processing event {} type: {}", event_idx, tm_event.kind);

                if let Ok(eureka_event) = EurekaEvent::try_from(tm_event.clone()) {
                    tracing::debug!(
                        "Successfully parsed Eureka event of type: {:?}",
                        std::mem::discriminant(&eureka_event)
                    );

                    match eureka_event {
                        EurekaEvent::SendPacket(packet) => {
                            tracing::debug!(
                                "Parsed send_packet: seq={}, src={}, dst={}",
                                packet.sequence,
                                packet.sourceClient,
                                packet.destClient
                            );

                            let payloads = packet
                                .payloads
                                .into_iter()
                                .map(|sol_payload| Payload {
                                    source_port: sol_payload.sourcePort,
                                    dest_port: sol_payload.destPort,
                                    version: sol_payload.version,
                                    encoding: sol_payload.encoding,
                                    value: sol_payload.value.to_vec(),
                                })
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
                    tracing::debug!(
                        "Failed to parse event {} as Eureka event: type={}",
                        event_idx,
                        tm_event.kind
                    );

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
                        _ => {
                            tracing::debug!("Unsupported event type: {}", tm_event.kind);
                        }
                    }
                }
            }
        }

        tracing::debug!(
            "Total Cosmos events fetched from {} transactions: {}",
            tx_hashes.len(),
            events.len()
        );
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

    /// Shared helper to process events and build instructions
    #[allow(clippy::cognitive_complexity)]
    pub(crate) async fn build_instructions_from_events<F, Fut>(
        &self,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
        update_client_builder: F,
    ) -> Result<Vec<Instruction>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Instruction>>,
    {
        let mut instructions = Vec::new();

        // First, update the Tendermint light client on Solana using provided builder
        let update_client_ix = update_client_builder().await?;
        instructions.push(update_client_ix);

        // Process source events from Cosmos
        for event in src_events {
            match event {
                CosmosIbcEvent::SendPacket {
                    sequence,
                    source_client,
                    destination_client,
                    payloads,
                    timeout_timestamp,
                } => {
                    tracing::debug!("Building recv packet instruction for sequence {}", sequence);
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
                    tracing::debug!("Building acknowledgement instruction");
                }
                CosmosIbcEvent::TimeoutPacket { .. } => {
                    tracing::debug!("Building timeout instruction");
                }
            }
        }

        for event in target_events {
            tracing::debug!(?event, "Processing timeout event");
        }

        Ok(instructions)
    }

    /// Shared helper to build transaction from instructions
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No instructions provided
    /// - Failed to get recent blockhash
    pub(crate) fn build_transaction_from_instructions(
        &self,
        instructions: &[Instruction],
    ) -> Result<Transaction> {
        if instructions.is_empty() {
            anyhow::bail!("No instructions to execute on Solana");
        }

        // Create unsigned transaction
        let mut tx = Transaction::new_with_payer(instructions, Some(&self.fee_payer));

        // Get recent blockhash for the transaction
        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        tx.message.recent_blockhash = recent_blockhash;

        Ok(tx)
    }

    /// Build a Solana transaction from IBC events
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client instruction
    /// - No instructions to execute
    pub async fn build_solana_tx(
        &self,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
    ) -> Result<Transaction> {
        tracing::debug!("Building Solana transaction with real (non-mock) logic");

        let instructions = self
            .build_instructions_from_events(src_events, target_events, || {
                self.build_update_client_instruction()
            })
            .await?;
        self.build_transaction_from_instructions(&instructions)
    }

    /// Resolve the program ID for a given port by querying the router's port registry
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to derive IBC app account
    /// - Failed to fetch account data
    /// - Failed to deserialize `IBCApp` account
    fn resolve_port_program_id(&self, port_id: &str) -> Result<Pubkey> {
        // Derive the IBCApp account PDA for this port
        let (ibc_app_account, _) = derive_ibc_app(port_id, &self.solana_ics26_program_id);

        // Fetch the account data from Solana
        let account_data = self
            .solana_client
            .get_account_data(&ibc_app_account)
            .map_err(|e| {
                anyhow::anyhow!("Failed to fetch IBCApp account for port '{}': {e}", port_id)
            })?;

        // Anchor accounts have an 8-byte discriminator prefix
        if account_data.len() < 8 {
            return Err(anyhow::anyhow!("Account data too short for IBCApp account"));
        }

        let account_data_without_discriminator = &account_data[8..];

        // IMPORTANT: We must use field-by-field deserialization because the on-chain account
        // has #[max_len(128)] which reserves 128 bytes for port_id. When port_id is shorter
        // (e.g., "transfer" = 12 bytes), the account contains padding that try_from_slice
        // would fail on with "Not all bytes read". Field deserializers handle this correctly.
        let mut remaining_data = account_data_without_discriminator;

        let port_id_from_account = <String as AnchorDeserialize>::deserialize(&mut remaining_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize port_id: {}", e))?;
        let app_program_id = <Pubkey as AnchorDeserialize>::deserialize(&mut remaining_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize app_program_id: {}", e))?;
        let authority = <Pubkey as AnchorDeserialize>::deserialize(&mut remaining_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize authority: {}", e))?;

        // Note: remaining_data still has ~120 bytes of padding from #[max_len(128)]

        let ibc_app = IBCApp {
            port_id: port_id_from_account,
            app_program_id,
            authority,
        };

        tracing::info!(
            "Resolved port '{}' to program ID: {}",
            port_id,
            ibc_app.app_program_id
        );

        Ok(ibc_app.app_program_id)
    }

    /// Build instruction to update Tendermint light client on Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get latest block from Cosmos
    /// - Failed to serialize header
    async fn build_update_client_instruction(&self) -> Result<Instruction> {
        tracing::info!("Building REGULAR update client instruction!");

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
    fn build_recv_packet_instruction(&self, params: &RecvPacketParams<'_>) -> Result<Instruction> {
        // Build the packet structure (IBC v2)
        let payloads = params.payloads.to_vec();

        // Get dest_port for PDA derivation before moving packet
        let dest_port = if payloads.is_empty() {
            return Err(anyhow::anyhow!("Payloads are empty"));
        } else {
            payloads[0].dest_port.clone() // Use actual destination port from first payload
        };

        // Make sure all payloads have the same destination port
        // TODO: Support multiple payload destinations
        for payload in &payloads {
            if payload.dest_port != dest_port {
                return Err(anyhow::anyhow!("Payloads have different destination ports"));
            }
        }

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

        // Resolve the actual IBC app program ID for this port
        let ibc_app_program_id = self.resolve_port_program_id(&dest_port)?;

        // Derive the app state account for the resolved IBC app
        let (ibc_app_state, _) = derive_app_state(&dest_port, &ibc_app_program_id);

        // Build accounts list in the correct order for RecvPacket struct
        let accounts = vec![
            AccountMeta::new_readonly(router_state, false), // router_state
            AccountMeta::new_readonly(ibc_app, false),      // ibc_app
            AccountMeta::new(client_sequence, false),       // client_sequence
            AccountMeta::new(packet_receipt, false),        // packet_receipt
            AccountMeta::new(packet_ack, false),            // packet_ack
            AccountMeta::new_readonly(ibc_app_program_id, false), // ibc_app_program
            AccountMeta::new(ibc_app_state, false),         // ibc_app_state
            AccountMeta::new_readonly(self.solana_ics26_program_id, false), // router_program
            AccountMeta::new_readonly(self.fee_payer, true), // relayer (signer)
            AccountMeta::new(self.fee_payer, true),         // payer (signer)
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false), // system_program
            AccountMeta::new_readonly(sysvar::clock::id(), false), // clock
            AccountMeta::new_readonly(client, false),       // client
            AccountMeta::new_readonly(self.solana_ics07_program_id, false), // light_client_program
            AccountMeta::new_readonly(client_state, false), // client_state
            AccountMeta::new_readonly(consensus_state, false), // consensus_state
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

    /// Build an update client transaction for Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get latest block
    /// - Failed to serialize header
    pub async fn build_update_client_tx(&self, client_id: String) -> Result<Transaction> {
        tracing::info!(
            "Building update client transaction for client {}",
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

/// Mock `TxBuilder` for testing that uses mock proofs and minimal data
pub struct MockTxBuilder {
    /// The underlying real `TxBuilder`
    pub inner: TxBuilder,
}

impl MockTxBuilder {
    /// Creates a new `MockTxBuilder`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `TxBuilder` creation fails.
    pub fn new(
        source_tm_client: HttpClient,
        solana_client: Arc<RpcClient>,
        solana_ics26_program_id: Pubkey,
        solana_ics07_program_id: Pubkey,
        fee_payer: Pubkey,
    ) -> anyhow::Result<Self> {
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

    /// Fetch events from Cosmos transactions
    ///
    /// # Errors
    ///
    /// Returns an error if fetching events fails.
    pub async fn fetch_cosmos_events(
        &self,
        tx_hashes: Vec<Hash>,
    ) -> anyhow::Result<Vec<CosmosIbcEvent>> {
        self.inner.fetch_cosmos_events(tx_hashes).await
    }

    /// Fetch timeout events from Solana
    ///
    /// # Errors
    ///
    /// Returns an error if fetching timeout events fails.
    pub fn fetch_solana_timeout_events(
        &self,
        tx_signatures: Vec<Signature>,
    ) -> anyhow::Result<Vec<CosmosIbcEvent>> {
        self.inner.fetch_solana_timeout_events(tx_signatures)
    }

    /// Build Solana transaction with mock behavior for testing
    ///
    /// # Errors
    ///
    /// Returns an error if building the transaction fails.
    pub async fn build_solana_tx(
        &self,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
    ) -> anyhow::Result<Transaction> {
        tracing::debug!("Building Solana transaction with MOCK logic");

        // Build instructions using shared logic with mock builders
        let instructions = self
            .inner
            .build_instructions_from_events(src_events, target_events, || async {
                self.build_update_client_instruction_mock()
            })
            .await?;

        // Use shared helper to build transaction
        let tx = self
            .inner
            .build_transaction_from_instructions(&instructions)?;

        let tx_size = bincode::serialize(&tx)?.len();
        tracing::debug!(
            "Mock transaction size: {} bytes (should be much smaller than 1644 bytes)",
            tx_size
        );

        Ok(tx)
    }

    /// Build create client transaction
    ///
    /// # Errors
    ///
    /// Returns an error if building the create client transaction fails.
    pub async fn build_create_client_tx(&self) -> anyhow::Result<Transaction> {
        self.inner.build_create_client_tx().await
    }

    /// Build update client transaction
    ///
    /// # Errors
    ///
    /// Returns an error if building the update client transaction fails.
    pub async fn build_update_client_tx(&self, client_id: String) -> anyhow::Result<Transaction> {
        self.inner.build_update_client_tx(client_id).await
    }

    /// Build mock update client instruction for testing
    fn build_update_client_instruction_mock(&self) -> anyhow::Result<Instruction> {
        tracing::debug!("Building MOCK update client instruction - should be tiny!");

        // Create a minimal mock UpdateClientMsg with empty data to avoid size issues
        let update_msg = UpdateClientMsg {
            client_message: b"mock_client_message".to_vec(), // Minimal mock data
        };

        // Get instruction discriminator for "update_client"
        let discriminator = get_instruction_discriminator("update_client");
        let mut data = discriminator.to_vec();
        data.extend_from_slice(&update_msg.try_to_vec()?);

        tracing::debug!("Mock update client instruction: discriminator={} bytes, update_msg={} bytes, total_data={} bytes",
            discriminator.len(), update_msg.try_to_vec()?.len(), data.len());

        Ok(Instruction {
            program_id: self.inner.solana_ics07_program_id,
            accounts: vec![
                // Mock accounts - these don't need to be real for testing
                AccountMeta::new(self.inner.fee_payer, false), // Mock client state
                AccountMeta::new(self.inner.fee_payer, false), // Mock trusted consensus state
                AccountMeta::new(self.inner.fee_payer, false), // Mock new consensus state
                AccountMeta::new(self.inner.fee_payer, true),  // Payer
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false), // System program
            ],
            data,
        })
    }
}
