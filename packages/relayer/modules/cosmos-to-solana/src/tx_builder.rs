//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use anchor_lang::AnchorSerialize;
use ibc_proto_eureka::ibc::core::client::v1::Height;
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

use crate::anchor_types::{
    derive_client, derive_client_sequence, derive_ibc_app, derive_ics07_client_state,
    derive_ics07_consensus_state, derive_packet_ack, derive_packet_receipt, derive_router_state,
    get_instruction_discriminator, ics07_program_id, ics26_program_id, MsgRecvPacket, Packet,
    Payload, UpdateClientMsg,
};

/// IBC event types from Cosmos
#[derive(Debug, Clone)]
pub enum CosmosIbcEvent {
    /// Send packet event
    SendPacket {
        /// Packet sequence
        sequence: u64,
        /// Source port
        source_port: String,
        /// Source channel (maps to client in Solana)
        source_client: String,
        /// Destination port
        destination_port: String,
        /// Destination channel (maps to client in Solana)
        destination_client: String,
        /// Packet data
        data: Vec<u8>,
        /// Timeout height
        _timeout_height: Height,
        /// Timeout timestamp
        timeout_timestamp: u64,
    },
    /// Acknowledge packet event
    AcknowledgePacket {
        /// Packet sequence
        sequence: u64,
        /// Source port
        source_port: String,
        /// Source channel
        source_client: String,
        /// Acknowledgement data
        acknowledgement: Vec<u8>,
    },
    /// Timeout packet event
    TimeoutPacket {
        /// Packet sequence
        sequence: u64,
        /// Source port
        source_port: String,
        /// Source channel
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
            solana_ics26_program_id: ics26_program_id(),
            solana_ics07_program_id: ics07_program_id(),
            wallet_keypair,
        })
    }

    /// Fetch events from Cosmos transactions
    ///
    /// # Errors
    ///
    /// Returns an error if failed to fetch Cosmos transaction
    pub async fn fetch_cosmos_events(&self, tx_hashes: Vec<Hash>) -> Result<Vec<CosmosIbcEvent>> {
        let mut events = Vec::new();

        for tx_hash in tx_hashes {
            // Fetch transaction from Tendermint
            let tx_result = self
                .source_tm_client
                .tx(tx_hash, false)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch Cosmos transaction: {e}"))?;

            // Parse IBC events from transaction result
            for event in tx_result.tx_result.events {
                match event.kind.as_str() {
                    "send_packet" => {
                        // Parse SendPacket event attributes
                        let mut sequence = 0u64;
                        let mut source_port = String::new();
                        let mut source_channel = String::new();
                        let mut destination_port = String::new();
                        let mut destination_channel = String::new();
                        let mut data = Vec::new();
                        let timeout_height = Height::default();
                        let mut timeout_timestamp = 0u64;

                        for attr in event.attributes {
                            match attr.key_str().unwrap_or("") {
                                "packet_sequence" => {
                                    sequence = attr.value_str().unwrap_or("0").parse().unwrap_or(0);
                                }
                                "packet_src_port" => {
                                    source_port = attr.value_str().unwrap_or("").to_string();
                                }
                                "packet_src_channel" => {
                                    source_channel = attr.value_str().unwrap_or("").to_string();
                                }
                                "packet_dst_port" => {
                                    destination_port = attr.value_str().unwrap_or("").to_string();
                                }
                                "packet_dst_channel" => {
                                    destination_channel = attr.value_str().unwrap_or("").to_string();
                                }
                                "packet_data_hex" => {
                                    data = hex::decode(attr.value_str().unwrap_or(""))
                                        .unwrap_or_default();
                                }
                                "packet_timeout_timestamp" => {
                                    timeout_timestamp =
                                        attr.value_str().unwrap_or("0").parse().unwrap_or(0);
                                }
                                _ => {}
                            }
                        }

                        // Map channel to client (in Solana, we use client IDs instead of channels)
                        // This is a simplification - in production, you'd map properly
                        let source_client = format!("cosmos-{}", source_channel);
                        let destination_client = format!("solana-{}", destination_channel);

                        events.push(CosmosIbcEvent::SendPacket {
                            sequence,
                            source_port,
                            source_client,
                            destination_port,
                            destination_client,
                            data,
                            _timeout_height: timeout_height,
                            timeout_timestamp,
                        });
                    }
                    "acknowledge_packet" => {
                        // Parse AcknowledgePacket event
                        tracing::debug!("Found acknowledge_packet event");
                    }
                    "timeout_packet" => {
                        // Parse TimeoutPacket event
                        tracing::debug!("Found timeout_packet event");
                    }
                    _ => {}
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
    #[allow(clippy::cognitive_complexity)]
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
                #[allow(clippy::used_underscore_binding)]
                CosmosIbcEvent::SendPacket {
                    sequence,
                    source_port,
                    source_client,
                    destination_port,
                    destination_client,
                    data,
                    _timeout_height,
                    timeout_timestamp,
                } => {
                    // Build RecvPacket instruction for Solana
                    let recv_packet_ix = self.build_recv_packet_instruction(
                        sequence,
                        &source_port,
                        &source_client,
                        &destination_port,
                        &destination_client,
                        &data,
                        timeout_timestamp,
                    )?;
                    instructions.push(recv_packet_ix);
                }
                CosmosIbcEvent::AcknowledgePacket { .. } => {
                    // Build Acknowledgement instruction
                    tracing::debug!("Building acknowledgement instruction");
                }
                CosmosIbcEvent::TimeoutPacket { .. } => {
                    // Build Timeout instruction
                    tracing::debug!("Building timeout instruction");
                }
            }
        }

        // Process timeout events from Solana
        for event in target_events {
            tracing::debug!("Processing timeout event: {:?}", event);
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
        let (client_state_pda, _) = derive_ics07_client_state(&chain_id);

        // Derive consensus state PDAs for trusted and new heights
        let trusted_height = latest_block.block.header.height.value() - 1; // Previous height
        let new_height = latest_block.block.header.height.value();

        let (trusted_consensus_state, _) =
            derive_ics07_consensus_state(&client_state_pda, trusted_height);
        let (new_consensus_state, _) = derive_ics07_consensus_state(&client_state_pda, new_height);

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
    fn build_recv_packet_instruction(
        &self,
        sequence: u64,
        source_port: &str,
        source_client: &str,
        destination_port: &str,
        destination_client: &str,
        data: &[u8],
        timeout_timestamp: u64,
    ) -> Result<Instruction> {
        // Build the packet structure
        let packet = Packet {
            sequence,
            source_client: source_client.to_string(),
            dest_client: destination_client.to_string(),
            timeout_timestamp: timeout_timestamp as i64,
            payloads: vec![Payload {
                source_port: source_port.to_string(),
                dest_port: destination_port.to_string(),
                version: "ics20-1".to_string(), // Default version
                encoding: "json".to_string(),
                value: data.to_vec(),
            }],
        };

        // Create the message
        let msg = MsgRecvPacket {
            packet: packet.clone(),
            proof_commitment: vec![], // Would include actual proof
            proof_height: 0,           // Would include actual height
        };

        // Derive all required PDAs
        let (router_state, _) = derive_router_state();
        let (ibc_app, _) = derive_ibc_app(destination_port);
        let (client_sequence, _) = derive_client_sequence(destination_client);
        let (packet_receipt, _) = derive_packet_receipt(destination_client, sequence);
        let (packet_ack, _) = derive_packet_ack(destination_client, sequence);
        let (client, _) = derive_client(destination_client);

        // For light client verification, we also need ICS07 accounts
        let (client_state, _) = derive_ics07_client_state(source_client);
        let (consensus_state, _) = derive_ics07_consensus_state(&client_state, 0); // Use appropriate height

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
        let (client_state_pda, _) = derive_ics07_client_state(&chain_id);
        let (consensus_state_pda, _) = derive_ics07_consensus_state(&client_state_pda, latest_height);

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
        tracing::info!("Building update client transaction for client {}", client_id);

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