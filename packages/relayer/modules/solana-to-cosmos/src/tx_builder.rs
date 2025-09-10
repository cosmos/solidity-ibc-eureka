//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Solana.

use std::collections::HashMap;
use std::sync::Arc;

use anchor_lang::prelude::*;
use ibc_eureka_relayer_lib::{
    events::solana::{parse_events_from_logs, IbcEvent},
    utils::cosmos,
};
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
            ClientMessage, ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        },
    },
};
use prost::Message;
use solana_client::rpc_client::RpcClient;
use solana_ibc_types::Packet as SolanaPacket;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};
use tendermint_rpc::HttpClient;

/// Mock header data for Solana client testing
const MOCK_HEADER_DATA: &[u8] = b"mock";

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
        payloads: Vec<Payload>,
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
        payloads: Vec<Payload>,
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
        payloads: Vec<Payload>,
        /// Timeout timestamp
        timeout_timestamp: u64,
    },
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
    #[allow(clippy::cognitive_complexity)]
    pub fn fetch_solana_events(
        &self,
        tx_signatures: &[Signature],
    ) -> anyhow::Result<Vec<SolanaIbcEvent>> {
        let mut events = Vec::new();

        for signature in tx_signatures {
            let tx = self
                .solana_client
                .get_transaction(signature, UiTransactionEncoding::Json)
                .map_err(|e| anyhow::anyhow!("Failed to fetch Solana transaction: {e}"))?;

            // Check if transaction was successful
            if let Some(ref meta) = tx.transaction.meta {
                if let Some(ref err) = meta.err {
                    tracing::debug!(
                        "Transaction {} failed with error: {:?}, skipping",
                        signature,
                        err
                    );
                    continue;
                }
                tracing::debug!("Transaction {} was successful", signature);
            } else {
                tracing::debug!("Transaction {} has no metadata, skipping", signature);
                continue;
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
        let logs = match &meta.log_messages {
            solana_transaction_status::option_serializer::OptionSerializer::Some(logs) => logs,
            _ => &empty_logs,
        };

        // Use the shared event parser - it now returns fully decoded events
        let parsed_events = parse_events_from_logs(logs);

        for event in parsed_events {
            match event {
                IbcEvent::SendPacket(send_event) => {
                    // Deserialize the packet to get full details
                    if let Ok(packet) = SolanaPacket::try_from_slice(&send_event.packet_data) {
                        let payloads: Vec<Payload> = packet
                            .payloads
                            .into_iter()
                            .map(convert_solana_payload_to_ibc)
                            .collect();

                        events.push(SolanaIbcEvent::SendPacket {
                            sequence: send_event.sequence,
                            source_client: packet.source_client.clone(),
                            destination_client: packet.dest_client.clone(),
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                        });
                    }
                }
                IbcEvent::AckPacket(ack_event) => {
                    // For acknowledge packet, we need the full packet data
                    if let Ok(packet) = SolanaPacket::try_from_slice(&ack_event.packet_data) {
                        let payloads: Vec<Payload> = packet
                            .payloads
                            .into_iter()
                            .map(convert_solana_payload_to_ibc)
                            .collect();

                        events.push(SolanaIbcEvent::AcknowledgePacket {
                            sequence: ack_event.sequence,
                            source_client: packet.source_client,
                            destination_client: packet.dest_client,
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                            acknowledgements: vec![ack_event.acknowledgement],
                        });
                    }
                }
                IbcEvent::TimeoutPacket(timeout_event) => {
                    // For timeout packet, we need the full packet data
                    if let Ok(packet) = SolanaPacket::try_from_slice(&timeout_event.packet_data) {
                        let payloads: Vec<Payload> = packet
                            .payloads
                            .into_iter()
                            .map(convert_solana_payload_to_ibc)
                            .collect();

                        events.push(SolanaIbcEvent::TimeoutPacket {
                            sequence: timeout_event.sequence,
                            source_client: packet.source_client,
                            destination_client: packet.dest_client,
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                        });
                    }
                }
                IbcEvent::WriteAcknowledgement(write_ack_event) => {
                    // WriteAcknowledgement is emitted when Solana receives a packet from Cosmos
                    // and writes an acknowledgement. We need to relay this ack back to Cosmos.
                    if let Ok(packet) = SolanaPacket::try_from_slice(&write_ack_event.packet_data) {
                        let payloads: Vec<Payload> = packet
                            .payloads
                            .into_iter()
                            .map(convert_solana_payload_to_ibc)
                            .collect();

                        events.push(SolanaIbcEvent::AcknowledgePacket {
                            sequence: write_ack_event.sequence,
                            source_client: packet.source_client,
                            destination_client: packet.dest_client,
                            payloads,
                            timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                            acknowledgements: write_ack_event.acknowledgements,
                        });
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
        client_id: &str,
        src_events: Vec<SolanaIbcEvent>,
        target_events: Vec<SolanaIbcEvent>, // Timeout events from target
    ) -> anyhow::Result<TxBody> {
        let mut messages = Vec::new();

        // First, update the Solana light client on Cosmos
        let update_msg = self.build_update_client_msg(client_id)?;
        messages.push(Any::from_msg(&update_msg)?);

        // Process source events from Solana
        for event in src_events {
            let msg = self.build_message_from_event(event)?;
            messages.push(msg);
        }

        // Process target events from Cosmos (for timeouts)
        for event in target_events {
            tracing::debug!("Processing timeout event from Cosmos: {:?}", event);
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
        tracing::debug!("Building message from Solana event: {:?}", event);
        match event {
            SolanaIbcEvent::SendPacket {
                sequence,
                source_client,
                destination_client,
                payloads,
                timeout_timestamp,
            } => {
                tracing::debug!("Building recv packet msg - sequence: {}, source_client: {}, dest_client: {}, payloads: {} items, timeout: {}",
                    sequence, source_client, destination_client, payloads.len(), timeout_timestamp);

                self.build_recv_packet_msg(
                    sequence,
                    source_client,
                    destination_client,
                    payloads,
                    timeout_timestamp,
                )
            }
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
                &payloads,
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

    /// Build a `RecvPacket` message for Cosmos
    #[allow(clippy::cognitive_complexity)]
    fn build_recv_packet_msg(
        &self,
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Payload>,
        timeout_timestamp: u64,
    ) -> anyhow::Result<Any> {
        tracing::debug!("Payloads count: {}", payloads.len());

        let packet = Packet {
            sequence,
            source_client,
            destination_client,
            timeout_timestamp,
            payloads,
        };

        let msg = MsgRecvPacket {
            packet: Some(packet.clone()),
            proof_height: None,       // Will be filled by proof injection
            proof_commitment: vec![], // Mock proof for now
            signer: self.signer_address.clone(),
        };

        tracing::debug!(
            "Created RecvPacket message for sequence {} with signer: {}",
            sequence,
            self.signer_address
        );
        tracing::debug!("Packet details: {:?}", packet);
        Any::from_msg(&msg).map_err(Into::into)
    }

    /// Build an Acknowledgement message for Cosmos
    #[allow(clippy::cognitive_complexity)]
    fn build_acknowledgement_msg(
        &self,
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: &[Payload],
        timeout_timestamp: u64,
        acknowledgements: Vec<Vec<u8>>,
    ) -> anyhow::Result<Any> {
        let packet = Packet {
            sequence,
            source_client,
            destination_client,
            timeout_timestamp,
            payloads: payloads.to_vec(),
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

        Any::from_msg(&msg).map_err(Into::into)
    }

    /// Build a Timeout message for Cosmos
    fn build_timeout_msg(
        &self,
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Payload>,
        timeout_timestamp: u64,
    ) -> anyhow::Result<Any> {
        let packet = Packet {
            sequence,
            source_client,
            destination_client,
            timeout_timestamp,
            payloads,
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
    fn build_update_client_msg(&self, client_id: &str) -> anyhow::Result<MsgUpdateClient> {
        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        tracing::info!(slot, "Updating Solana client");

        // Create update message with latest Solana state
        // This would include proof-of-history verification data
        let header_data = MOCK_HEADER_DATA.to_vec(); // Mock Solana header for testing
        let client_msg = Any::from_msg(&ClientMessage { data: header_data })?;

        Ok(MsgUpdateClient {
            client_id: client_id.to_string(),
            client_message: Some(client_msg),
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
        parameters: &HashMap<String, String>,
    ) -> anyhow::Result<TxBody> {
        // For Solana, we would create a WASM light client on Cosmos
        // that can verify Solana's proof-of-history consensus

        tracing::info!("Creating Solana light client on Cosmos");

        // Get latest Solana slot/block information
        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        // Extract checksum from parameters
        let checksum_hex = parameters
            .get("checksum_hex")
            .ok_or_else(|| anyhow::anyhow!("Missing checksum_hex parameter"))?;

        let checksum = hex::decode(checksum_hex)
            .map_err(|e| anyhow::anyhow!("Failed to decode checksum hex: {e}"))?;

        // Create WASM client state for Solana verification with proper checksum
        // This would contain the Solana validator set and consensus parameters
        let client_state = WasmClientState {
            data: b"mock_client_state".to_vec(), // Mock Solana-specific client state
            checksum,                            // Use actual WASM code checksum from parameters
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
        let header_data = MOCK_HEADER_DATA.to_vec(); // Mock Solana header for testing
        let client_msg = Any::from_msg(&ClientMessage { data: header_data })?;

        let update_msg = MsgUpdateClient {
            client_id,
            client_message: Some(client_msg),
            signer: self.signer_address.clone(),
        };

        Ok(TxBody {
            messages: vec![Any::from_msg(&update_msg)?],
            ..Default::default()
        })
    }
}

/// Mock `TxBuilder` for testing that uses mock proofs instead of real ones
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
    pub fn build_relay_tx(
        &self,
        client_id: &str,
        src_events: Vec<SolanaIbcEvent>,
        target_events: Vec<SolanaIbcEvent>,
    ) -> anyhow::Result<TxBody> {
        // Build transaction normally first
        let mut tx = self
            .inner
            .build_relay_tx(client_id, src_events, target_events)?;

        // Then inject mock proofs into the transaction
        Self::inject_mock_proofs_into_tx(&mut tx)?;

        Ok(tx)
    }

    /// Build create client transaction
    ///
    /// # Errors
    ///
    /// Returns an error if building the create client transaction fails.
    pub fn build_create_client_tx(
        &self,
        parameters: &HashMap<String, String>,
    ) -> anyhow::Result<TxBody> {
        self.inner.build_create_client_tx(parameters)
    }

    /// Build update client transaction
    ///
    /// # Errors
    ///
    /// Returns an error if building the update client transaction fails.
    pub fn build_update_client_tx(&self, client_id: String) -> anyhow::Result<TxBody> {
        self.inner.build_update_client_tx(client_id)
    }

    /// Injects mock proofs into IBC messages in the transaction for testing purposes.
    fn inject_mock_proofs_into_tx(tx: &mut TxBody) -> anyhow::Result<()> {
        use ibc_proto_eureka::ibc::core::channel::v2::{
            MsgAcknowledgement, MsgRecvPacket, MsgTimeout,
        };

        // Collect all messages by type
        let mut recv_msgs = Vec::new();
        let mut ack_msgs = Vec::new();
        let mut timeout_msgs = Vec::new();
        let mut other_msgs = Vec::new();

        for any_msg in &tx.messages {
            match any_msg.type_url.as_str() {
                url if url.contains("MsgRecvPacket") => {
                    recv_msgs.push(MsgRecvPacket::decode(any_msg.value.as_slice())?);
                }
                url if url.contains("MsgAcknowledgement") => {
                    ack_msgs.push(MsgAcknowledgement::decode(any_msg.value.as_slice())?);
                }
                url if url.contains("MsgTimeout") => {
                    timeout_msgs.push(MsgTimeout::decode(any_msg.value.as_slice())?);
                }
                _ => {
                    other_msgs.push(any_msg.clone()); // Keep non-IBC messages as-is
                }
            }
        }

        // Apply mock proofs to all messages at once
        cosmos::inject_mock_proofs(&mut recv_msgs, &mut ack_msgs, &mut timeout_msgs);

        // Rebuild the transaction with updated messages
        tx.messages.clear();
        tx.messages.extend(other_msgs);
        tx.messages.extend(
            recv_msgs
                .into_iter()
                .map(|msg| Any::from_msg(&msg).unwrap()),
        );
        tx.messages
            .extend(ack_msgs.into_iter().map(|msg| Any::from_msg(&msg).unwrap()));
        tx.messages.extend(
            timeout_msgs
                .into_iter()
                .map(|msg| Any::from_msg(&msg).unwrap()),
        );

        Ok(())
    }
}

fn convert_solana_payload_to_ibc(solana_payload: solana_ibc_types::Payload) -> Payload {
    Payload {
        source_port: solana_payload.source_port,
        destination_port: solana_payload.dest_port,
        version: solana_payload.version,
        encoding: solana_payload.encoding,
        value: solana_payload.value,
    }
}
