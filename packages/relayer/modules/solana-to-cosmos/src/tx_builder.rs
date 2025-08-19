//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Solana.

use std::collections::HashMap;

use anyhow::Result;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use ibc_proto_eureka::{
    cosmos::tx::v1beta1::TxBody,
    google::protobuf::Any,
    ibc::{
        core::client::v1::{Height, MsgCreateClient, MsgUpdateClient},
        lightclients::wasm::v1::{
            ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        },
    },
};
use prost::Message;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::UiTransactionEncoding;
use tendermint_rpc::HttpClient;

/// IBC event types emitted by Solana programs
#[derive(Debug, Clone)]
pub enum SolanaIbcEvent {
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
        timeout_height: Height,
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
        /// Destination port
        destination_port: String,
        /// Destination channel
        destination_channel: String,
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
        /// Destination port
        destination_port: String,
        /// Destination channel
        destination_channel: String,
    },
}

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from Solana.
#[allow(dead_code)]
pub struct TxBuilder {
    /// The Solana RPC client.
    pub solana_client: RpcClient,
    /// The HTTP client for the target chain.
    pub target_tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: Pubkey,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub const fn new(
        solana_client: RpcClient,
        target_tm_client: HttpClient,
        signer_address: String,
        solana_ics26_program_id: Pubkey,
    ) -> Self {
        Self {
            solana_client,
            target_tm_client,
            signer_address,
            solana_ics26_program_id,
        }
    }

    /// Fetch events from Solana transactions
    pub async fn fetch_solana_events(
        &self,
        tx_signatures: Vec<Signature>,
    ) -> Result<Vec<SolanaIbcEvent>> {
        let mut events = Vec::new();

        for signature in tx_signatures {
            let tx = self
                .solana_client
                .get_transaction(&signature, UiTransactionEncoding::Json)
                .map_err(|e| anyhow::anyhow!("Failed to fetch Solana transaction: {}", e))?;

            // Check if transaction was successful
            if tx
                .transaction
                .meta
                .as_ref()
                .map_or(true, |m| m.err.is_some())
            {
                continue; // Skip failed transactions
            }

            // Parse logs for IBC events
            if let Some(meta) = tx.transaction.meta {
                for log in meta.log_messages.unwrap_or_default() {
                    // Parse SendPacket events
                    if log.contains("Program log: IBC SendPacket") {
                        // This is a simplified parsing - in production you'd parse the actual event data
                        // from the instruction data or return data
                        tracing::debug!("Found SendPacket event in Solana logs");

                        // In a real implementation, you would:
                        // 1. Parse the instruction data from the transaction
                        // 2. Decode the Anchor/Borsh serialized event data
                        // 3. Extract packet details
                    }
                    // Parse other event types similarly
                    if log.contains("Program log: IBC AcknowledgePacket") {
                        tracing::debug!("Found AcknowledgePacket event in Solana logs");
                    }
                    if log.contains("Program log: IBC TimeoutPacket") {
                        tracing::debug!("Found TimeoutPacket event in Solana logs");
                    }
                }
            }
        }

        Ok(events)
    }

    /// Build a relay transaction for Cosmos
    pub async fn build_relay_tx(
        &self,
        src_events: Vec<SolanaIbcEvent>,
        target_events: Vec<SolanaIbcEvent>, // Timeout events from target
    ) -> Result<TxBody> {
        let mut messages = Vec::new();

        // First, update the Solana light client on Cosmos
        let update_msg = self.build_update_client_msg().await?;
        messages.push(Any::from_msg(&update_msg)?);

        // Process source events from Solana
        for event in src_events {
            match event {
                SolanaIbcEvent::SendPacket { .. } => {
                    // Create RecvPacket message for Cosmos
                    // This would involve creating the proper message type with proofs
                    tracing::debug!("Processing SendPacket event from Solana");
                }
                SolanaIbcEvent::AcknowledgePacket { .. } => {
                    // Create Acknowledgement message for Cosmos
                    tracing::debug!("Processing AcknowledgePacket event from Solana");
                }
                SolanaIbcEvent::TimeoutPacket { .. } => {
                    // Create Timeout message for Cosmos
                    tracing::debug!("Processing TimeoutPacket event from Solana");
                }
            }
        }

        // Process target events from Cosmos (for timeouts)
        for event in target_events {
            tracing::debug!("Processing timeout event from Cosmos: {:?}", event);
            // Process timeout events
        }

        if messages.is_empty() {
            anyhow::bail!("No messages to relay to Cosmos");
        }

        Ok(TxBody {
            messages,
            ..Default::default()
        })
    }

    /// Build an update client message for the Solana light client on Cosmos
    async fn build_update_client_msg(&self) -> Result<MsgUpdateClient> {
        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {}", e))?;

        tracing::info!("Updating Solana client at slot {}", slot);

        // Create update message with latest Solana state
        // This would include proof-of-history verification data
        Ok(MsgUpdateClient {
            client_id: "08-wasm-0".to_string(), // Example client ID for WASM light client
            client_message: Some(Any {
                type_url: "/ibc.lightclients.wasm.v1.Header".to_string(),
                value: vec![], // Serialize Solana header with PoH proof
            }),
            signer: self.signer_address.clone(),
        })
    }

    /// Build a create client transaction
    pub async fn build_create_client_tx(
        &self,
        parameters: HashMap<String, String>,
    ) -> Result<TxBody> {
        // For Solana, we would create a WASM light client on Cosmos
        // that can verify Solana's proof-of-history consensus

        tracing::info!("Creating Solana light client on Cosmos");

        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {}", e))?;

        // Create WASM client state for Solana verification
        // This would contain the Solana validator set and consensus parameters
        let client_state = WasmClientState {
            data: vec![],     // Serialize Solana-specific client state
            checksum: vec![], // WASM code checksum for Solana light client
            latest_height: Some(Height {
                revision_number: 0, // Solana doesn't have revision numbers
                revision_height: slot,
            }),
        };

        // Create consensus state with current Solana state
        let consensus_state = WasmConsensusState {
            data: vec![], // Serialize Solana-specific consensus state (PoH, validators, etc.)
        };

        let create_msg = MsgCreateClient {
            client_state: Some(Any::from_msg(&client_state)?),
            consensus_state: Some(Any::from_msg(&consensus_state)?),
            signer: self.signer_address.clone(),
        };

        Ok(TxBody {
            messages: vec![Any::from_msg(&create_msg)?],
            ..Default::default()
        })
    }

    /// Build an update client transaction
    pub async fn build_update_client_tx(&self, client_id: String) -> Result<TxBody> {
        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {}", e))?;

        tracing::info!("Updating Solana client {} at slot {}", client_id, slot);

        // Create update message with latest Solana state
        // This would include proof-of-history verification data
        let update_msg = MsgUpdateClient {
            client_id,
            client_message: Some(Any {
                type_url: "/ibc.lightclients.wasm.v1.Header".to_string(),
                value: vec![], // Serialize Solana header with PoH proof
            }),
            signer: self.signer_address.clone(),
        };

        Ok(TxBody {
            messages: vec![Any::from_msg(&update_msg)?],
            ..Default::default()
        })
    }
}

