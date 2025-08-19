//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

use std::collections::HashMap;

use anyhow::Result;
use ibc_proto_eureka::ibc::core::client::v1::Height;
use std::sync::Arc;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use solana_transaction_status::UiTransactionEncoding;
use tendermint::Hash;
use tendermint_rpc::{Client, HttpClient};

/// IBC event types from Cosmos
#[derive(Debug, Clone)]
pub enum CosmosIbcEvent {
    /// Send packet event
    SendPacket {
        /// Packet sequence
        sequence: u64,
        /// Source port
        source_port: String,
        /// Source channel
        source_channel: String,
        /// Destination port
        destination_port: String,
        /// Destination channel
        destination_channel: String,
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
        source_channel: String,
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
        source_channel: String,
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
        solana_ics26_program_id: Pubkey,
        solana_ics07_program_id: Pubkey,
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
            solana_ics26_program_id,
            solana_ics07_program_id,
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
            let tx_result = self.source_tm_client
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
                                    data = hex::decode(attr.value_str().unwrap_or("")).unwrap_or_default();
                                }
                                "packet_timeout_timestamp" => {
                                    timeout_timestamp = attr.value_str().unwrap_or("0").parse().unwrap_or(0);
                                }
                                _ => {}
                            }
                        }

                        events.push(CosmosIbcEvent::SendPacket {
                            sequence,
                            source_port,
                            source_channel,
                            destination_port,
                            destination_channel,
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
            let tx = self
                .solana_client
                .get_transaction(&signature, UiTransactionEncoding::Json)
                .map_err(|e| anyhow::anyhow!("Failed to fetch Solana transaction: {e}"))?;

            // Parse timeout events from Solana transaction logs
            if let Some(_meta) = tx.transaction.meta {
                // In Solana 2.0, log_messages is serialized differently
                // In production, you'd parse the actual instruction data instead of logs
                // For now, this is a placeholder implementation
                tracing::debug!("Processing Solana transaction metadata for timeouts");
            }
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
                    source_channel,
                    destination_port,
                    destination_channel,
                    data,
                    _timeout_height,
                    timeout_timestamp,
                } => {
                    // Build RecvPacket instruction for Solana
                    let recv_packet_ix = self.build_recv_packet_instruction(
                        sequence,
                        &source_port,
                        &source_channel,
                        &destination_port,
                        &destination_channel,
                        &data,
                        _timeout_height,
                        timeout_timestamp,
                    );
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
        let recent_blockhash = self.solana_client
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
        let latest_block = self.source_tm_client
            .latest_block()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get latest block: {e}"))?;

        // Serialize the header and validators for the update
        let header_bytes = bincode::serialize(&latest_block.block.header)
            .map_err(|e| anyhow::anyhow!("Failed to serialize header: {e}"))?;

        // Build the instruction data
        let mut instruction_data = vec![0]; // Instruction discriminator for UpdateClient
        instruction_data.extend_from_slice(&header_bytes);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts: vec![
                AccountMeta::new(self.wallet_keypair.pubkey(), true), // Signer
                // Add other required accounts for light client update
            ],
            data: instruction_data,
        })
    }

    /// Build instruction for `RecvPacket` on Solana
    /// 
    /// # Errors
    /// 
    /// Returns an error if packet data cannot be serialized
    #[allow(clippy::too_many_arguments, clippy::used_underscore_binding)]
    fn build_recv_packet_instruction(
        &self,
        sequence: u64,
        source_port: &str,
        source_channel: &str,
        destination_port: &str,
        destination_channel: &str,
        data: &[u8],
        _timeout_height: Height,
        timeout_timestamp: u64,
    ) -> Instruction {
        // Build the instruction data for RecvPacket
        let mut instruction_data = vec![1]; // Instruction discriminator for RecvPacket
        instruction_data.extend_from_slice(&sequence.to_le_bytes());
        instruction_data.extend_from_slice(source_port.as_bytes());
        instruction_data.extend_from_slice(source_channel.as_bytes());
        instruction_data.extend_from_slice(destination_port.as_bytes());
        instruction_data.extend_from_slice(destination_channel.as_bytes());
        instruction_data.extend_from_slice(data);
        instruction_data.extend_from_slice(&timeout_timestamp.to_le_bytes());

        Instruction {
            program_id: self.solana_ics26_program_id,
            accounts: vec![
                AccountMeta::new(self.wallet_keypair.pubkey(), true), // Signer
                // Add other required accounts for packet reception
            ],
            data: instruction_data,
        }
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

        let genesis_block = self.source_tm_client
            .block(u32::try_from(genesis_height).unwrap_or(1))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get genesis block: {e}"))?;

        // Build instruction data for creating Tendermint light client
        let mut instruction_data = vec![2]; // Instruction discriminator for CreateClient
        let header_bytes = bincode::serialize(&genesis_block.block.header)
            .map_err(|e| anyhow::anyhow!("Failed to serialize header: {e}"))?;
        instruction_data.extend_from_slice(&header_bytes);

        let instruction = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts: vec![
                AccountMeta::new(self.wallet_keypair.pubkey(), true), // Signer
                // Add other required accounts
            ],
            data: instruction_data,
        };

        // Get recent blockhash
        let recent_blockhash = self.solana_client
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

        // Get latest block from Cosmos
        let latest_block = self.source_tm_client
            .latest_block()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get latest block: {e}"))?;

        // Build instruction data
        let mut instruction_data = vec![3]; // Instruction discriminator for UpdateClient
        instruction_data.extend_from_slice(client_id.as_bytes());
        let header_bytes = bincode::serialize(&latest_block.block.header)
            .map_err(|e| anyhow::anyhow!("Failed to serialize header: {e}"))?;
        instruction_data.extend_from_slice(&header_bytes);

        let instruction = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts: vec![
                AccountMeta::new(self.wallet_keypair.pubkey(), true), // Signer
                // Add other required accounts
            ],
            data: instruction_data,
        };

        // Get recent blockhash
        let recent_blockhash = self.solana_client
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
}