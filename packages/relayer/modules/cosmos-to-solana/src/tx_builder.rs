//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use anchor_lang::AnchorSerialize;
use anyhow::Result;
use ibc_eureka_relayer_lib::events::EurekaEvent;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    sysvar,
    transaction::Transaction,
};
use tendermint::Hash;
use tendermint_rpc::{Client, HttpClient};

use solana_ibc_types::{
    derive_client, derive_client_sequence, derive_ibc_app, derive_ics07_client_state,
    derive_ics07_consensus_state, derive_packet_ack, derive_packet_receipt, derive_router_state,
    get_instruction_discriminator, MsgRecvPacket, Packet, Payload, UpdateClientMsg,
};

use solana_ibc_constants::{ICS07_TENDERMINT_ID, ICS26_ROUTER_ID};

/// Parameters for building a `RecvPacket` instruction
struct RecvPacketParams<'a> {
    sequence: u64,
    source_client: &'a str,
    destination_client: &'a str,
    payloads: &'a [Vec<u8>],
    timeout_timestamp: u64,
}

// TODO: Move out?
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
    /// The Solana wallet keypair for signing transactions.
    pub wallet_keypair: Keypair,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to read wallet file
    /// - Failed to parse wallet JSON
    /// - Failed to create keypair from wallet
    pub fn new(
        source_tm_client: HttpClient,
        solana_client: Arc<RpcClient>,
        _solana_ics26_program_id: Pubkey, // Use hardcoded for now
        _solana_ics07_program_id: Pubkey, // Use hardcoded for now
        wallet_path: &str,
    ) -> Result<Self> {
        // Load wallet keypair from file
        let wallet_json = std::fs::read_to_string(wallet_path)
            .map_err(|e| anyhow::anyhow!("Failed to read wallet file: {e}"))?;
        let wallet_bytes: Vec<u8> = serde_json::from_str(&wallet_json)
            .map_err(|e| anyhow::anyhow!("Failed to parse wallet JSON: {e}"))?;
        let wallet_keypair = Keypair::try_from(wallet_bytes.as_slice())
            .map_err(|e| anyhow::anyhow!("Failed to create keypair: {e}"))?;

        Ok(Self {
            source_tm_client,
            solana_client,
            solana_ics26_program_id: Pubkey::from_str(ICS26_ROUTER_ID)
                .map_err(|e| anyhow::anyhow!("Invalid ICS26 program ID: {e}"))?,
            solana_ics07_program_id: Pubkey::from_str(ICS07_TENDERMINT_ID)
                .map_err(|e| anyhow::anyhow!("Invalid ICS07 program ID: {e}"))?,
            wallet_keypair,
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

    /// Build a Solana transaction from IBC events
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client instruction
    /// - No instructions to execute
    /// - Failed to get latest blockhash
    #[allow(clippy::cognitive_complexity)] // TODO: Refactor when all event types are fully implemented
    pub async fn build_solana_tx(
        &self,
        src_events: Vec<CosmosIbcEvent>,
        target_events: Vec<CosmosIbcEvent>,
    ) -> Result<Transaction> {
        let mut instructions = Vec::new();

        // First, update the Tendermint light client on Solana
        let update_client_ix = self.build_update_client_instruction().await?;
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

        if instructions.is_empty() {
            anyhow::bail!("No instructions to execute on Solana");
        }

        // Get recent blockhash
        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        // Create and sign transaction
        let tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.wallet_keypair.pubkey()),
            &[&self.wallet_keypair],
            recent_blockhash,
        );

        Ok(tx)
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

        // For ICS07 Tendermint, we need to properly serialize the header
        // This is a simplified version - in production, you'd use proper Tendermint types
        let header_bytes = serde_json::to_vec(&latest_block.block.header)
            .map_err(|e| anyhow::anyhow!("Failed to serialize header: {e}"))?;

        let update_msg = UpdateClientMsg {
            header: header_bytes,
        };

        // Get the chain ID for PDA derivation
        let chain_id = latest_block.block.header.chain_id.to_string();
        let (client_state_pda, _) =
            derive_ics07_client_state(&chain_id, &self.solana_ics07_program_id);

        // Derive consensus state PDAs for trusted and new heights
        let trusted_height = latest_block.block.header.height.value() - 1; // Previous height
        let new_height = latest_block.block.header.height.value();

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
            AccountMeta::new(self.wallet_keypair.pubkey(), true),
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
            timeout_timestamp: i64::try_from(params.timeout_timestamp).unwrap_or(i64::MAX),
            payloads,
        };

        // Create the message with mock proofs for now
        let msg = MsgRecvPacket {
            packet,
            proof_commitment: b"mock".to_vec(), // Mock proof for testing
            proof_height: 1,                    // Mock height for testing
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
            AccountMeta::new_readonly(self.wallet_keypair.pubkey(), true), // relayer
            AccountMeta::new(self.wallet_keypair.pubkey(), true),          // payer
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
    /// - Failed to serialize header
    /// - Failed to get latest blockhash
    pub async fn build_create_client_tx(
        &self,
        parameters: HashMap<String, String>,
    ) -> Result<Transaction> {
        // Get genesis block from Cosmos for initial client state
        let genesis_height = parameters
            .get("genesis_height")
            .and_then(|h| h.parse::<i64>().ok())
            .unwrap_or(1);

        let genesis_block = self
            .source_tm_client
            .block(u32::try_from(genesis_height).unwrap_or(1))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get genesis block: {e}"))?;

        // In production, you'd properly serialize the client state and consensus state
        // This is simplified
        let chain_id = genesis_block.block.header.chain_id.to_string();
        let latest_height = genesis_block.block.header.height.value();

        // Derive PDAs
        let (client_state_pda, _) =
            derive_ics07_client_state(&chain_id, &self.solana_ics07_program_id);
        let (consensus_state_pda, _) = derive_ics07_consensus_state(
            &client_state_pda,
            latest_height,
            &self.solana_ics07_program_id,
        );

        // Build initialization instruction for ICS07
        let accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(consensus_state_pda, false),
            AccountMeta::new(self.wallet_keypair.pubkey(), true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = get_instruction_discriminator("initialize");
        let mut data = discriminator.to_vec();
        // Add serialized initialization parameters
        data.extend_from_slice(chain_id.as_bytes());

        let instruction = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        // Get recent blockhash
        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        // Create and sign transaction
        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.wallet_keypair.pubkey()),
            &[&self.wallet_keypair],
            recent_blockhash,
        );

        Ok(tx)
    }

    /// Build an update client transaction for Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get latest block
    /// - Failed to serialize header
    /// - Failed to get latest blockhash
    pub async fn build_update_client_tx(&self, client_id: String) -> Result<Transaction> {
        tracing::info!(
            "Building update client transaction for client {}",
            client_id
        );

        let update_ix = self.build_update_client_instruction().await?;

        // Get recent blockhash
        let recent_blockhash = self
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        // Create and sign transaction
        let tx = Transaction::new_signed_with_payer(
            &[update_ix],
            Some(&self.wallet_keypair.pubkey()),
            &[&self.wallet_keypair],
            recent_blockhash,
        );

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
        wallet_path: &str,
    ) -> Result<Self> {
        Ok(Self {
            inner: TxBuilder::new(
                source_tm_client,
                solana_client,
                solana_ics26_program_id,
                solana_ics07_program_id,
                wallet_path,
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
            timeout_timestamp: i64::try_from(params.timeout_timestamp).unwrap_or(i64::MAX),
            payloads,
        };

        // Create the message with mock proofs
        let msg = MsgRecvPacket {
            packet,
            proof_commitment: b"mock".to_vec(), // Mock proof
            proof_height: 1,                    // Mock height
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
            AccountMeta::new_readonly(self.inner.wallet_keypair.pubkey(), true), // relayer
            AccountMeta::new(self.inner.wallet_keypair.pubkey(), true),          // payer
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
    /// - Failed to get latest blockhash
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

        // Get recent blockhash
        let recent_blockhash = self
            .inner
            .solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))?;

        // Create and sign transaction
        let tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.inner.wallet_keypair.pubkey()),
            &[&self.inner.wallet_keypair],
            recent_blockhash,
        );

        Ok(tx)
    }
}
