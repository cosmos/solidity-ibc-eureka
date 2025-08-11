//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from an Attested chain via the aggregator.

use std::collections::{HashMap, HashSet};

use alloy::sol_types::SolValue;
use anyhow::Result;
use attestor_light_client::header::Header;
use ibc_proto_eureka::{
    cosmos::tx::v1beta1::TxBody,
    google::protobuf::Any,
    ibc::{core::client::v1::MsgUpdateClient, lightclients::wasm::v1::ClientMessage},
};
use prost::Message;
use tendermint_rpc::HttpClient;
use tonic::transport::Channel;

use ibc_eureka_relayer_lib::{
    chain::{Chain, CosmosSdk},
    events::{EurekaEvent, EurekaEventWithHeight},
    tx_builder::TxBuilderService,
    utils::{attestor, cosmos},
};
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;

/// Chain type for attested chains that get their state from the aggregator
pub struct AttestedChain;

impl Chain for AttestedChain {
    type Event = EurekaEventWithHeight;
    type TxId = String;
    type Height = u64;
}

/// Generated aggregator client protobuf definitions
pub mod aggregator_proto {
    tonic::include_proto!("aggregator");
}

/// The aggregator client for fetching attestations.
pub type AggregatorClient =
    aggregator_proto::aggregator_service_client::AggregatorServiceClient<Channel>;

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on attestations from the aggregator.
pub struct TxBuilder {
    /// The aggregator URL for fetching attestations.
    pub aggregator_url: String,
    /// The HTTP client for the target chain.
    pub target_tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub fn new(
        aggregator_url: String,
        target_tm_client: HttpClient,
        signer_address: String,
    ) -> Self {
        Self {
            aggregator_url,
            target_tm_client,
            signer_address,
        }
    }

    /// Creates an aggregator client.
    async fn create_aggregator_client(&self) -> Result<AggregatorClient> {
        let channel = Channel::from_shared(self.aggregator_url.clone())?
            .connect()
            .await?;
        Ok(AggregatorClient::new(channel))
    }
}

fn encode_and_cyphon_packet_if_relevant(
    packet: &Packet,
    cyphon: &mut Vec<Vec<u8>>,
    src_client_id: &str,
    dst_client_id: &str,
    seqs: &[u64],
) {
    if packet.sourceClient == src_client_id
        && packet.destClient == dst_client_id
        && (seqs.is_empty() || seqs.contains(&packet.sequence))
    {
        cyphon.push(packet.abi_encode())
    }
}

#[async_trait::async_trait]
impl TxBuilderService<AttestedChain, CosmosSdk> for TxBuilder {
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        target_events: Vec<EurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<u8>> {
        tracing::info!(
            "Building relay transaction from aggregator for {} source events and {} timeout events",
            src_events.len(),
            target_events.len()
        );

        let mut aggregator_client = self.create_aggregator_client().await?;

        let mut ics26_send_packets = Vec::new();
        let mut ics26_ack_packets = Vec::new();
        let mut heights = HashSet::new();

        for event in src_events.iter() {
            // Prepare cyphon and filtering params
            let (packet, cyphon, seqs) = match event.event {
                EurekaEvent::SendPacket(ref packet) => {
                    (packet, &mut ics26_send_packets, &src_packet_seqs)
                }
                EurekaEvent::WriteAcknowledgement(ref packet, _) => {
                    (packet, &mut ics26_ack_packets, &dst_packet_seqs)
                }
            };
            heights.insert(event.height);
            encode_and_cyphon_packet_if_relevant(
                packet,
                cyphon,
                &src_client_id,
                &dst_client_id,
                seqs,
            );
        }

        let query_height = *heights.iter().max().unwrap();
        let request = aggregator_proto::GetAttestationsRequest {
            packets: ics26_send_packets,
            height: query_height, // latest height
        };

        tracing::info!(
            "Requesting state attestation from aggregator for {} packets",
            request.packets.len()
        );
        // - We need update client with timestamp at height
        // - We need MsgRecvPacket where proof is (key, value, height, signature)
        let response = aggregator_client
            .get_attestations(request)
            .await?
            .into_inner();

        let (state, packets) = (
            response
                .state_attestation
                .ok_or(anyhow::anyhow!("No state received"))?,
            response
                .packet_attestation
                .ok_or(anyhow::anyhow!("No packets received"))?,
        );
        tracing::info!(
            "Received state attestation: {} signatures, height {}, state: {}",
            packets.signatures.len(),
            packets.height,
            hex::encode(&packets.attested_data)
        );

        let header = Header::new(
            state.height,
            state.timestamp.unwrap(),
            state.attested_data,
            state.signatures,
            state.public_keys,
        );
        let header_bz = serde_json::to_vec(&header)
            .map_err(|_| anyhow::anyhow!("header could not be serialized"))?;

        let update_msg = MsgUpdateClient {
            client_id: dst_client_id.clone(),
            client_message: Some(Any::from_msg(&ClientMessage { data: header_bz })?),
            signer: self.signer_address.clone(),
        };

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let mut timeout_msgs = cosmos::target_events_to_timeout_msgs(
            target_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix.as_secs(),
        );

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix.as_secs(),
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        attestor::inject_proofs(&mut recv_msgs, &packets.attested_data, packets.height);

        // We don't want to use mock proofs for RecvMsg
        let mut dummy = [];
        cosmos::inject_mock_proofs(&dummy, &mut ack_msgs, &mut timeout_msgs);

        let all_msgs = timeout_msgs
            .into_iter()
            .map(|m| Any::from_msg(&m))
            .chain(recv_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(ack_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain([Any::from_msg(&update_msg)])
            .collect::<Result<Vec<_>, _>>()?;

        tracing::debug!("Total messages: #{}", all_msgs.len());

        let tx_body = TxBody {
            messages: all_msgs,
            ..Default::default()
        };

        tracing::debug!("TX to send {:?}", tx_body);

        Ok(tx_body.encode_to_vec())
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(&self, _parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        tracing::info!("Creating attested light client");

        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(&self, dst_client_id: String) -> Result<Vec<u8>> {
        tracing::info!("Updating attested light client: {}", dst_client_id);

        todo!()
    }
}
