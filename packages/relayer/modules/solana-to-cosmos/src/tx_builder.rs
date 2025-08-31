//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Solana.

use std::collections::HashMap;
use std::sync::Arc;

use anchor_lang::prelude::*;
use ibc_eureka_relayer_lib::events::solana::{parse_events_from_logs, IbcEvent};
use ibc_eureka_relayer_lib::events::EurekaEvent;
use ibc_proto_eureka::{
    cosmos::tx::v1beta1::TxBody,
    google::protobuf::Any,
    ibc::{
        core::{
            channel::v2::{
                Acknowledgement, MsgAcknowledgement, MsgRecvPacket, MsgTimeout, Packet, Payload,
            },
            client::v1::{Height, MsgCreateClient, MsgUpdateClient},
        },
        lightclients::wasm::v1::{
            ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        },
    },
};
use solana_client::rpc_client::RpcClient;
use solana_ibc_types::Packet as SolanaPacket;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};
use tendermint::Hash;
use tendermint_rpc::{Client, HttpClient};

/// IBC event types emitted by Solana programs
#[derive(Debug, Clone)]
pub enum SolanaIbcEvent {
    /// Send packet event
    SendPacket {
        /// Packet sequence
        sequence: u64,
        /// Source client ID
        source_client: String,
        /// Destination client ID
        destination_client: String,
        /// Packet payloads (IBC v2 supports multiple payloads)
        payloads: Vec<Vec<u8>>,
        /// Timeout timestamp
        timeout_timestamp: u64,
    },
    /// Acknowledge packet event
    AcknowledgePacket {
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
        /// Acknowledgement data (one per payload)
        acknowledgements: Vec<Vec<u8>>,
    },
    /// Timeout packet event
    TimeoutPacket {
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
}

/// Timeout event from Cosmos
#[derive(Debug, Clone)]
pub struct CosmosTimeoutEvent {
    /// Packet sequence
    pub sequence: u64,
    /// Source client ID
    pub source_client: String,
    /// Destination client ID
    pub destination_client: String,
    /// Packet payloads
    pub payloads: Vec<Vec<u8>>,
    /// Timeout timestamp
    pub timeout_timestamp: u64,
}

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from Solana.
#[allow(dead_code)]
pub struct TxBuilder {
    /// The Solana RPC client
    pub solana_client: Arc<RpcClient>,
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
        solana_client: Arc<RpcClient>,
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
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch Solana transaction
    /// - Transaction deserialization fails
    /// - Event parsing fails
    pub fn fetch_solana_events(
        &self,
        tx_signatures: Vec<Signature>,
    ) -> anyhow::Result<Vec<SolanaIbcEvent>> {
        let mut events = Vec::new();

        for signature in tx_signatures {
            let tx = self
                .solana_client
                .get_transaction(&signature, UiTransactionEncoding::Json)
                .map_err(|e| anyhow::anyhow!("Failed to fetch Solana transaction: {e}"))?;

            // Check if transaction was successful
            if tx.transaction.meta.as_ref().is_none_or(|m| m.err.is_some()) {
                continue; // Skip failed transactions
            }

            // Parse logs for IBC events
            if let Some(ref meta) = tx.transaction.meta {
                // Parse events from transaction logs
                let parsed_events = Self::parse_events_from_logs(&tx, meta);
                events.extend(parsed_events);
            }
        }

        Ok(events)
    }

    /// Parse IBC events from Solana transaction logs
    ///
    /// Uses the shared event parsing utilities from solana-ibc-types
    #[allow(clippy::cognitive_complexity)] // Event parsing is inherently complex
    fn parse_events_from_logs(
        _tx: &EncodedConfirmedTransactionWithStatusMeta,
        meta: &solana_transaction_status::UiTransactionStatusMeta,
    ) -> Vec<SolanaIbcEvent> {
        let mut events = Vec::new();

        // Get logs from the transaction
        let empty_logs = vec![];
        let logs = meta.log_messages.as_ref().unwrap_or(&empty_logs);

        // Use the shared event parser - it now returns fully decoded events
        let parsed_events = parse_events_from_logs(logs);

        for event in parsed_events {
            match event {
                IbcEvent::SendPacket(send_event) => {
                    // Deserialize the packet to get full details
                    if let Ok(packet) = SolanaPacket::try_from_slice(&send_event.packet_data) {
                        let payloads = packet.payloads.into_iter().map(|p| p.value).collect();

                        events.push(SolanaIbcEvent::SendPacket {
                            sequence: send_event.sequence,
                            source_client: packet.source_client.clone(),
                            destination_client: packet.dest_client.clone(),
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                        });

                        tracing::debug!(
                            "Parsed SendPacket event: seq={}, src={}, dst={}",
                            send_event.sequence,
                            packet.source_client,
                            packet.dest_client
                        );
                    }
                }
                IbcEvent::AckPacket(ack_event) => {
                    // For acknowledge packet, we need the full packet data
                    if let Ok(packet) = SolanaPacket::try_from_slice(&ack_event.packet_data) {
                        let payloads = packet.payloads.into_iter().map(|p| p.value).collect();

                        events.push(SolanaIbcEvent::AcknowledgePacket {
                            sequence: ack_event.sequence,
                            source_client: packet.source_client,
                            destination_client: packet.dest_client,
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                            acknowledgements: vec![ack_event.acknowledgement],
                        });

                        tracing::debug!(
                            "Parsed AcknowledgePacket event: seq={}, client={}",
                            ack_event.sequence,
                            ack_event.client_id
                        );
                    }
                }
                IbcEvent::TimeoutPacket(timeout_event) => {
                    // For timeout packet, we need the full packet data
                    if let Ok(packet) = SolanaPacket::try_from_slice(&timeout_event.packet_data) {
                        let payloads = packet.payloads.into_iter().map(|p| p.value).collect();

                        events.push(SolanaIbcEvent::TimeoutPacket {
                            sequence: timeout_event.sequence,
                            source_client: packet.source_client,
                            destination_client: packet.dest_client,
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                        });

                        tracing::debug!(
                            "Parsed TimeoutPacket event: seq={}, client={}",
                            timeout_event.sequence,
                            timeout_event.client_id
                        );
                    }
                }
                IbcEvent::WriteAcknowledgement(write_ack_event) => {
                    // WriteAcknowledgement is emitted when Solana receives a packet from Cosmos
                    // and writes an acknowledgement. We need to relay this ack back to Cosmos.
                    if let Ok(packet) = SolanaPacket::try_from_slice(&write_ack_event.packet_data) {
                        let payloads = packet.payloads.into_iter().map(|p| p.value).collect();

                        events.push(SolanaIbcEvent::AcknowledgePacket {
                            sequence: write_ack_event.sequence,
                            source_client: packet.source_client,
                            destination_client: packet.dest_client,
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                            acknowledgements: vec![write_ack_event.acknowledgements],
                        });

                        tracing::debug!(
                            "Parsed WriteAcknowledgement event: seq={}, client={}",
                            write_ack_event.sequence,
                            write_ack_event.client_id
                        );
                    }
                }
            }
        }

        events
    }

    /// Build a relay transaction for Cosmos
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client message
    /// - No messages to relay
    pub fn build_relay_tx(
        &self,
        src_events: Vec<SolanaIbcEvent>,
        target_events: Vec<CosmosTimeoutEvent>, // Timeout events from target
    ) -> anyhow::Result<TxBody> {
        let mut messages = Vec::new();

        // First, update the Solana light client on Cosmos
        let update_msg = self.build_update_client_msg()?;
        messages.push(Any::from_msg(&update_msg)?);

        // Process source events from Solana
        for event in src_events {
            let msg = self.build_message_from_event(event)?;
            messages.push(msg);
        }

        // Process target timeout events from Cosmos
        // These are packets that timed out on Cosmos and need timeout confirmation on Solana
        for event in target_events {
            tracing::debug!("Processing timeout event from Cosmos: {:?}", event);
            // Build timeout message to relay back to source chain
            let timeout_msg = self.build_timeout_msg(
                event.sequence,
                event.source_client,
                event.destination_client,
                event.payloads,
                event.timeout_timestamp,
            )?;
            messages.push(timeout_msg);
        }

        if messages.len() == 1 {
            // Only contains the update client message
            anyhow::bail!("No IBC messages to relay to Cosmos");
        }

        Ok(TxBody {
            messages,
            ..Default::default()
        })
    }

    /// Build a Cosmos message from a Solana IBC event
    fn build_message_from_event(&self, event: SolanaIbcEvent) -> anyhow::Result<Any> {
        match event {
            SolanaIbcEvent::SendPacket {
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
            } => self.build_recv_packet_msg(
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
            ),
            SolanaIbcEvent::AcknowledgePacket {
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
                acknowledgements,
            } => self.build_acknowledgement_msg(
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
                acknowledgements,
            ),
            SolanaIbcEvent::TimeoutPacket {
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
            } => self.build_timeout_msg(
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
            ),
        }
    }

    /// Convert Solana payloads to IBC v2 Payload format
    fn convert_payloads_to_ibc(payloads: Vec<Vec<u8>>) -> Vec<Payload> {
        payloads
            .into_iter()
            .map(|value| Payload {
                source_port: "transfer".to_string(), // Default for ICS20
                destination_port: "transfer".to_string(),
                version: "ics20-1".to_string(),
                encoding: "json".to_string(),
                value,
            })
            .collect()
    }

    /// Build a `RecvPacket` message for Cosmos
    fn build_recv_packet_msg(
        &self,
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Vec<u8>>,
        timeout_timestamp: u64,
    ) -> anyhow::Result<Any> {
        let packet = Packet {
            sequence,
            source_client,
            destination_client,
            timeout_timestamp,
            payloads: Self::convert_payloads_to_ibc(payloads),
        };

        let msg = MsgRecvPacket {
            packet: Some(packet),
            proof_height: None,       // Will be filled by proof injection
            proof_commitment: vec![], // Mock proof for now
            signer: self.signer_address.clone(),
        };

        tracing::debug!("Created RecvPacket message for sequence {}", sequence);
        Any::from_msg(&msg).map_err(Into::into)
    }

    /// Build an Acknowledgement message for Cosmos
    fn build_acknowledgement_msg(
        &self,
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Vec<u8>>,
        timeout_timestamp: u64,
        acknowledgements: Vec<Vec<u8>>,
    ) -> anyhow::Result<Any> {
        let packet = Packet {
            sequence,
            source_client,
            destination_client,
            timeout_timestamp,
            payloads: Self::convert_payloads_to_ibc(payloads),
        };

        let ack = Acknowledgement {
            app_acknowledgements: acknowledgements,
        };

        let msg = MsgAcknowledgement {
            packet: Some(packet),
            acknowledgement: Some(ack),
            proof_height: None,  // Will be filled by proof injection
            proof_acked: vec![], // Mock proof for now
            signer: self.signer_address.clone(),
        };

        tracing::debug!("Created Acknowledgement message for sequence {}", sequence);
        Any::from_msg(&msg).map_err(Into::into)
    }

    /// Build a Timeout message for Cosmos
    fn build_timeout_msg(
        &self,
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Vec<u8>>,
        timeout_timestamp: u64,
    ) -> anyhow::Result<Any> {
        let packet = Packet {
            sequence,
            source_client,
            destination_client,
            timeout_timestamp,
            payloads: Self::convert_payloads_to_ibc(payloads),
        };

        let msg = MsgTimeout {
            packet: Some(packet),
            proof_height: None,       // Will be filled by proof injection
            proof_unreceived: vec![], // Mock proof for now
            signer: self.signer_address.clone(),
        };

        tracing::debug!("Created Timeout message for sequence {}", sequence);
        Any::from_msg(&msg).map_err(Into::into)
    }

    /// Build an update client message for the Solana light client on Cosmos
    ///
    /// # Errors
    ///
    /// Returns an error if failed to get Solana slot
    fn build_update_client_msg(&self) -> anyhow::Result<MsgUpdateClient> {
        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        tracing::info!(slot, "Updating Solana client");

        // Create update message with latest Solana state
        // This would include proof-of-history verification data
        Ok(MsgUpdateClient {
            client_id: "08-wasm-0".to_string(), // Example client ID for WASM light client
            client_message: Some(Any {
                type_url: "/ibc.lightclients.wasm.v1.Header".to_string(),
                value: b"mock".to_vec(), // Mock Solana header for testing
            }),
            signer: self.signer_address.clone(),
        })
    }

    /// Build a create client transaction
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get Solana slot
    /// - Failed to serialize client state
    pub fn build_create_client_tx(
        &self,
        _parameters: HashMap<String, String>,
    ) -> anyhow::Result<TxBody> {
        // For Solana, we would create a WASM light client on Cosmos
        // that can verify Solana's proof-of-history consensus

        tracing::info!("Creating Solana light client on Cosmos");

        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        // Create WASM client state for Solana verification with mock data
        // This would contain the Solana validator set and consensus parameters
        let client_state = WasmClientState {
            data: b"mock_client_state".to_vec(), // Mock Solana-specific client state
            checksum: b"mock_checksum".to_vec(), // Mock WASM code checksum
            latest_height: Some(Height {
                revision_number: 0, // Solana doesn't have revision numbers
                revision_height: slot,
            }),
        };

        // Create consensus state with mock Solana state
        let consensus_state = WasmConsensusState {
            data: b"mock_consensus_state".to_vec(), // Mock Solana-specific consensus state
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

    /// Fetch timeout events from Cosmos transactions
    ///
    /// This fetches packets that have timed out on Cosmos and need to be relayed back to Solana.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch transaction from Cosmos
    /// - Failed to parse events
    ///
    /// # Panics
    ///
    /// This function should not panic under normal circumstances.
    pub async fn fetch_cosmos_timeout_events(
        &self,
        tx_hashes: Vec<Hash>,
    ) -> anyhow::Result<Vec<CosmosTimeoutEvent>> {
        let mut timeout_events = Vec::new();

        for tx_hash in tx_hashes {
            // Fetch transaction from Tendermint
            let tx_result = self
                .target_tm_client
                .tx(tx_hash, false)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch Cosmos transaction: {e}"))?;

            let _height = tx_result.height.value();

            // Parse events from the transaction
            for tm_event in tx_result.tx_result.events {
                // Check if this is a timeout packet event
                if tm_event.kind.as_str() == "timeout_packet" {
                    let mut sequence = None;
                    let mut source_client = None;
                    let mut dest_client = None;
                    let mut timeout_timestamp = None;
                    let mut packet_data = None;

                    // Parse attributes from the event
                    for attr in &tm_event.attributes {
                        match attr.key_str().unwrap_or("") {
                            "packet_sequence" => {
                                sequence =
                                    attr.value_str().ok().and_then(|s| s.parse::<u64>().ok());
                            }
                            "packet_src_client" | "source_client" => {
                                source_client = attr.value_str().ok().map(String::from);
                            }
                            "packet_dst_client" | "destination_client" => {
                                dest_client = attr.value_str().ok().map(String::from);
                            }
                            "packet_timeout_timestamp" | "timeout_timestamp" => {
                                timeout_timestamp =
                                    attr.value_str().ok().and_then(|s| s.parse::<u64>().ok());
                            }
                            "packet_data" => {
                                packet_data = attr.value_str().ok().map(String::from);
                            }
                            _ => {}
                        }
                    }

                    // If we have all required fields, create a timeout event
                    if let (Some(seq), Some(src), Some(dst), Some(timeout_ts)) =
                        (sequence, source_client, dest_client, timeout_timestamp)
                    {
                        // Try to decode packet data if available
                        let payloads = packet_data.map_or_else(
                            || vec![vec![]], // Empty payload if no data found
                            |data| {
                                // Attempt to decode as hex or base64
                                use base64::{engine::general_purpose::STANDARD, Engine as _};
                                hex::decode(&data)
                                    .map_or_else(
                                        |_| STANDARD.decode(&data).map_or_else(
                                            |_| vec![data.into_bytes()], // Fallback to raw bytes
                                            |decoded| vec![decoded]
                                        ),
                                        |decoded| vec![decoded]
                                    )
                            },
                        );

                        timeout_events.push(CosmosTimeoutEvent {
                            sequence: seq,
                            source_client: src,
                            destination_client: dst,
                            payloads,
                            timeout_timestamp: timeout_ts,
                        });

                        let last_event = timeout_events.last().expect("just pushed an event");
                        tracing::info!(
                            "Parsed timeout event: seq={}, src={}, dst={}, timeout={}",
                            seq,
                            last_event.source_client,
                            last_event.destination_client,
                            timeout_ts
                        );
                    }
                }
                // Also try to parse using the EurekaEvent enum if it supports timeout
                else if let Ok(_eureka_event) = EurekaEvent::try_from(tm_event.clone()) {
                    // Handle timeout if EurekaEvent supports it in the future
                }
            }
        }

        Ok(timeout_events)
    }

    /// Build an update client transaction
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get Solana slot
    /// - Failed to serialize update message
    pub fn build_update_client_tx(&self, client_id: String) -> anyhow::Result<TxBody> {
        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        tracing::info!(client_id, slot, "Updating Solana client");

        // Create update message with latest Solana state
        // This would include proof-of-history verification data
        let update_msg = MsgUpdateClient {
            client_id,
            client_message: Some(Any {
                type_url: "/ibc.lightclients.wasm.v1.Header".to_string(),
                value: b"mock".to_vec(), // Mock Solana header for testing
            }),
            signer: self.signer_address.clone(),
        };

        Ok(TxBody {
            messages: vec![Any::from_msg(&update_msg)?],
            ..Default::default()
        })
    }
}

/// Mock `TxBuilder` for testing that uses mock proofs and simplified event processing
pub struct MockTxBuilder {
    /// The underlying real `TxBuilder`
    pub inner: TxBuilder,
}

impl MockTxBuilder {
    /// Creates a new `MockTxBuilder`.
    #[must_use]
    pub const fn new(
        solana_client: Arc<RpcClient>,
        target_tm_client: HttpClient,
        signer_address: String,
        solana_ics26_program_id: Pubkey,
    ) -> Self {
        Self {
            inner: TxBuilder::new(
                solana_client,
                target_tm_client,
                signer_address,
                solana_ics26_program_id,
            ),
        }
    }

    /// Build a relay transaction for Cosmos with mock proofs
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client message
    /// - No messages to relay
    #[allow(clippy::cognitive_complexity)] // TODO: Refactor when implementing real proof generation
    pub fn build_relay_tx_mock(
        &self,
        src_events: Vec<SolanaIbcEvent>,
        target_events: Vec<CosmosTimeoutEvent>,
    ) -> anyhow::Result<TxBody> {
        let mut messages = Vec::new();

        // First, update the Solana light client on Cosmos with mock data
        let update_msg = self.build_update_client_msg_mock()?;
        messages.push(Any::from_msg(&update_msg)?);

        // Process source events from Solana with mock proofs
        for event in src_events {
            match event {
                SolanaIbcEvent::SendPacket { .. } => {
                    // In production, would create RecvPacket with real proofs
                    // For now, just log that we would process it
                    tracing::debug!("Processing SendPacket event from Solana with mock proof");
                }
                SolanaIbcEvent::AcknowledgePacket { .. } => {
                    tracing::debug!(
                        "Processing AcknowledgePacket event from Solana with mock proof"
                    );
                }
                SolanaIbcEvent::TimeoutPacket { .. } => {
                    tracing::debug!("Processing TimeoutPacket event from Solana with mock proof");
                }
            }
        }

        // Process target events from Cosmos (for timeouts)
        for event in target_events {
            tracing::debug!(
                "Processing timeout event from Cosmos with mock proof: {:?}",
                event
            );
        }

        if messages.is_empty() {
            anyhow::bail!("No messages to relay to Cosmos");
        }

        Ok(TxBody {
            messages,
            ..Default::default()
        })
    }

    /// Build an update client message with mock data
    ///
    /// # Errors
    ///
    /// Returns an error if failed to get Solana slot
    fn build_update_client_msg_mock(&self) -> anyhow::Result<MsgUpdateClient> {
        // Get latest Solana slot/block information
        let slot = self
            .inner
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        tracing::info!(slot, "Updating Solana client with mock data");

        // Create update message with mock Solana state
        Ok(MsgUpdateClient {
            client_id: "08-wasm-0".to_string(), // Example client ID for WASM light client
            client_message: Some(Any {
                type_url: "/ibc.lightclients.wasm.v1.Header".to_string(),
                value: b"mock_solana_header".to_vec(), // Mock header
            }),
            signer: self.inner.signer_address.clone(),
        })
    }
}
