//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from an Attested chain via the aggregator.

use std::collections::HashMap;

use alloy::sol_types::SolValue;
use anyhow::Result;
use ibc_proto_eureka::cosmos::tx::v1beta1::TxBody;
use prost::Message;
use tendermint_rpc::HttpClient;
use tonic::transport::Channel;

use ibc_eureka_relayer_lib::{
    chain::{Chain, CosmosSdk},
    events::{EurekaEvent, EurekaEventWithHeight},
    tx_builder::TxBuilderService,
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
pub type AggregatorClient = aggregator_proto::aggregator_client::AggregatorClient<Channel>;

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
    packet: Packet,
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

        let mut send_packets = Vec::new();
        let mut ack_packets = Vec::new();

        for event in src_events {
            // Prepare cyphon and filtering params
            let (packet, cyphon, seqs) = match event.event {
                EurekaEvent::SendPacket(packet) => (packet, &mut send_packets, &src_packet_seqs),
                EurekaEvent::WriteAcknowledgement(packet, _) => {
                    (packet, &mut ack_packets, &dst_packet_seqs)
                }
            };
            encode_and_cyphon_packet_if_relevant(
                packet,
                cyphon,
                &src_client_id,
                &dst_client_id,
                seqs,
            );
        }

        let request = aggregator_proto::GetStateAttestationRequest {
            packets: send_packets,
            height: 0, // latest height
        };

        tracing::info!(
            "Requesting state attestation from aggregator for {} packets",
            request.packets.len()
        );

        let response = aggregator_client
            .get_state_attestation(request)
            .await?
            .into_inner();

        tracing::info!(
            "Received state attestation: {} signatures, height {}, state: {}",
            response.sig_pubkey_pairs.len(),
            response.height,
            hex::encode(&response.state)
        );

        // TODO: Build actual cosmos transaction with the attestation data
        // This requires implementing the IBC client and packet message construction
        let tx_body = TxBody {
            messages: vec![],
            ..Default::default()
        };

        let serialized = tx_body.encode_to_vec();
        Ok(serialized)
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
