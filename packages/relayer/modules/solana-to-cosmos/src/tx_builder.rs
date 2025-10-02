//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Solana.

use prost::Message;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use ibc_eureka_relayer_lib::{
    chain::{CosmosSdk, SolanaEureka},
    events::{solana::SolanaEurekaEvent, EurekaEventWithHeight, SolanaEurekaEventWithHeight},
    tx_builder::TxBuilderService,
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
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use tendermint_rpc::HttpClient;

/// Mock header data for Solana client testing
const MOCK_HEADER_DATA: &[u8] = b"mock";

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from Solana.
#[allow(dead_code)]
pub struct MockTxBuilder {
    /// The Solana RPC client
    pub solana_client: Arc<RpcClient>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: Pubkey,
}

impl MockTxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub const fn new(
        solana_client: Arc<RpcClient>,
        tm_client: HttpClient,
        signer_address: String,
        solana_ics26_program_id: Pubkey,
    ) -> Self {
        Self {
            solana_client,
            tm_client,
            signer_address,
            solana_ics26_program_id,
        }
    }

    /// Build a relay transaction for Cosmos from Solana events
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to build update client message
    /// - No messages to relay
    pub fn build_relay_tx(
        &self,
        client_id: &str,
        src_events: Vec<SolanaEurekaEventWithHeight>,
        target_events: Vec<EurekaEventWithHeight>, // Timeout events from target
    ) -> anyhow::Result<TxBody> {
        unimplemented!()
        //     let mut messages = Vec::new();
        //
        //     // First, update the Solana light client on Cosmos
        //     let update_msg = self.build_update_client_msg(client_id)?;
        //     messages.push(Any::from_msg(&update_msg)?);
        //
        //     // Process source events from Solana
        //     for event_with_height in src_events {
        //         if let Some(msg) = self.build_message_from_event(event_with_height.event)? {
        //             messages.push(msg);
        //         }
        //     }
        //
        //     // Process target events from Cosmos (for timeouts)
        //     for event in target_events {
        //         tracing::debug!("Processing timeout event from Cosmos: {:?}", event);
        //     }
        //
        //     if messages.len() == 1 {
        //         // Only contains the update client message
        //         anyhow::bail!("No IBC messages to relay to Cosmos");
        //     }
        //
        //     Ok(TxBody {
        //         messages,
        //         ..Default::default()
        //     })
        // }
        //
        // /// Build a Cosmos message from a Solana IBC event
        // fn build_message_from_event(&self, event: SolanaEurekaEvent) -> anyhow::Result<Option<Any>> {
        //     tracing::info!("Building message from Solana event: {:?}", event);
        //     match event {
        //         SolanaEurekaEvent::SendPacket(send_event) => {
        //             let packet = &send_event.packet;
        //             let payloads = packet.payloads.iter().map(|p| p.value.clone()).collect();
        //
        //             tracing::info!("Building recv packet msg - sequence: {}, source_client: {}, dest_client: {}, payloads: {} items",
        //                 send_event.sequence, packet.source_client, packet.dest_client, packet.payloads.len());
        //
        //             let msg = self.build_recv_packet_msg(
        //                 send_event.sequence,
        //                 packet.source_client.clone(),
        //                 packet.dest_client.clone(),
        //                 payloads,
        //                 u64::try_from(packet.timeout_timestamp).unwrap_or(0),
        //             )?;
        //             Ok(Some(msg))
        //         }
        //         SolanaEurekaEvent::WriteAcknowledgement(write_ack) => {
        //             let packet = &write_ack.packet;
        //             let payloads = packet.payloads.iter().map(|p| p.value.clone()).collect();
        //
        //             let msg = self.build_acknowledgement_msg(
        //                 write_ack.sequence,
        //                 packet.source_client.clone(),
        //                 packet.dest_client.clone(),
        //                 payloads,
        //                 u64::try_from(packet.timeout_timestamp).unwrap_or(0),
        //                 vec![write_ack.acknowledgements],
        //             )?;
        //             Ok(Some(msg))
        //         }
        //         SolanaEurekaEvent::TimeoutPacket(timeout_event) => {
        //             let packet = &timeout_event.packet;
        //             let payloads = packet.payloads.iter().map(|p| p.value.clone()).collect();
        //
        //             let msg = self.build_timeout_msg(
        //                 timeout_event.sequence,
        //                 packet.source_client.clone(),
        //                 packet.dest_client.clone(),
        //                 payloads,
        //                 u64::try_from(packet.timeout_timestamp).unwrap_or(0),
        //             )?;
        //             Ok(Some(msg))
        //         }
        //         _ => {
        //             // Skip non-packet events
        //             tracing::debug!("Skipping non-packet event");
        //             Ok(None)
        //         }
        //     }
    }

    /// Convert Solana payloads to IBC v2 Payload format
    fn convert_payloads_to_ibc(payloads: Vec<Vec<u8>>) -> Vec<Payload> {
        payloads
            .into_iter()
            .map(|value| Payload {
                source_port: "transfer".to_string(), // Default for ICS20
                destination_port: "transfer".to_string(),
                version: "ics20-1".to_string(),
                encoding: "application/json".to_string(),
                value,
            })
            .collect()
    }

    /// Build a `RecvPacket` message for Cosmos
    #[allow(clippy::cognitive_complexity)]
    fn build_recv_packet_msg(
        &self,
        sequence: u64,
        source_client: String,
        destination_client: String,
        payloads: Vec<Vec<u8>>,
        timeout_timestamp: u64,
    ) -> anyhow::Result<Any> {
        let converted_payloads = Self::convert_payloads_to_ibc(payloads);
        tracing::info!("Converted payloads count: {}", converted_payloads.len());

        let packet = Packet {
            sequence,
            source_client,
            destination_client,
            timeout_timestamp,
            payloads: converted_payloads,
        };

        let msg = MsgRecvPacket {
            packet: Some(packet.clone()),
            proof_height: None,       // Will be filled by proof injection
            proof_commitment: vec![], // Mock proof for now
            signer: self.signer_address.clone(),
        };

        tracing::info!(
            "Created RecvPacket message for sequence {} with signer: {}",
            sequence,
            self.signer_address
        );
        tracing::info!("Packet details: {:?}", packet);
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
    fn build_update_client_msg(&self, client_id: &str) -> anyhow::Result<MsgUpdateClient> {
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

    pub fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        tracing::info!("Creating Solana light client on Cosmos");

        let slot = self
            .solana_client
            .get_slot()
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

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

        let tx = TxBody {
            messages: vec![Any::from_msg(&create_msg)?],
            ..Default::default()
        };

        Ok(tx.encode_to_vec())
    }

    /// Build an update client transaction
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to get Solana slot
    /// - Failed to serialize update message
    pub fn update_client(&self, client_id: String) -> anyhow::Result<Vec<u8>> {
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
        }
        .encode_to_vec())
    }
}

#[async_trait::async_trait]
impl TxBuilderService<SolanaEureka, CosmosSdk> for MockTxBuilder {
    async fn relay_events(
        &self,
        src_events: Vec<SolanaEurekaEventWithHeight>,
        dest_events: Vec<EurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> anyhow::Result<Vec<u8>> {
        tracing::info!(
            "Relaying events from Solana to Cosmos for client {}",
            dst_client_id
        );
        let now_since_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let mut timeout_msgs = cosmos::target_events_to_timeout_msgs(
            dest_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix,
        );

        // NOTE: Convert to eureka event to reuse to recvs/ack msg fn
        let src_events_as_sol_events = src_events
            .into_iter()
            .map(EurekaEventWithHeight::from)
            .collect();

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events_as_sol_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix,
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

    #[tracing::instrument(skip_all)]
    async fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        self.create_client(parameters)
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(&self, dst_client_id: String) -> anyhow::Result<Vec<u8>> {
        self.update_client(dst_client_id)
    }
}
