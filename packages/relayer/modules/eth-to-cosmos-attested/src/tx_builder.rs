//! This mod
//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from an Attested chain via the aggregator.

use std::collections::{HashMap, HashSet};

use alloy::sol_types::SolValue;
use alloy_primitives::Address;
use anyhow::Result;
use attestor_light_client::{
    client_state::ClientState as AttestorClientState,
    consensus_state::ConsensusState as AttestorConsensusState, header::Header,
    membership::MembershipProof,
};
use attestor_packet_membership::Packets;
use ibc_proto_eureka::{
    cosmos::tx::v1beta1::TxBody,
    google::protobuf::Any,
    ibc::{
        core::client::v1::{Height, MsgCreateClient, MsgUpdateClient},
        lightclients::wasm::v1::{
            ClientMessage, ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        },
    },
};
use k256::ecdsa::{Signature, VerifyingKey};
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
        cyphon.push(packet.abi_encode());
    }
}

const CHECKSUM_HEX: &str = "checksum_hex";
const PUB_KEYS: &str = "pub_keys";
const MIN_REQUIRED_SIGS: &str = "min_required_sigs";
const HEIGHT: &str = "height";
const TIMESTAMP: &str = "timestamp";

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

        for event in &src_events {
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

        let response = aggregator_client
            .get_attestations(request)
            .await?
            .into_inner();

        let (state, packets) = (
            response
                .state_attestation
                .ok_or_else(|| anyhow::anyhow!("No state received"))?,
            response
                .packet_attestation
                .ok_or_else(|| anyhow::anyhow!("No packets received"))?,
        );

        tracing::info!(
            "Received state attestation: {} signatures, height {}, state: {}",
            packets.signatures.len(),
            packets.height,
            hex::encode(&packets.attested_data)
        );

        let header = Header::new(
            state.height,
            // Unwrap safe as state attestation must contain ts
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

        let timeout_msgs = cosmos::target_events_to_timeout_msgs(
            target_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix.as_secs(),
        );

        let (mut recv_msgs, ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
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

        let proof = build_membership_proof_bytes(packets.attested_data, packets.signatures)?;
        attestor::inject_proofs_for_tm_msg(&mut recv_msgs, &proof, packets.height);

        // NOTE: UpdateMsg must come first otherwise
        // client state may not contain the needed
        // height for the RecvMsgs
        let all_msgs = std::iter::once(Any::from_msg(&update_msg))
            .chain(recv_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(timeout_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(ack_msgs.into_iter().map(|m| Any::from_msg(&m)))
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
    async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        let checksum_hex = parameters
            .get(CHECKSUM_HEX)
            .ok_or_else(|| anyhow::anyhow!(format!("Missing `{CHECKSUM_HEX}` parameter")))?;
        let checksum = alloy::hex::decode(checksum_hex)?;

        let min_required_sigs: u8 = parameters
            .get(MIN_REQUIRED_SIGS)
            .ok_or_else(|| anyhow::anyhow!(format!("Missing `{MIN_REQUIRED_SIGS}` parameter")))?
            .parse()?;

        let height: u64 = parameters
            .get(HEIGHT)
            .ok_or_else(|| anyhow::anyhow!(format!("Missing `{HEIGHT}` parameter")))?
            .parse()?;

        let timestamp: u64 = parameters
            .get(TIMESTAMP)
            .ok_or_else(|| anyhow::anyhow!(format!("Missing `{TIMESTAMP}` parameter")))?
            .parse()?;

        let pub_keys_hex = parameters
            .get(PUB_KEYS)
            .ok_or_else(|| anyhow::anyhow!(format!("Missing `{PUB_KEYS}` parameter")))?;
        let pub_keys_bytes = alloy::hex::decode(pub_keys_hex)?;
        anyhow::ensure!(
            pub_keys_bytes.len() % 33 == 0,
            "`{PUB_KEYS}` must be a hex-encoded concatenation of 33-byte compressed pubkeys"
        );
        let pub_keys: Vec<VerifyingKey> = pub_keys_bytes
            .chunks_exact(33)
            .map(VerifyingKey::from_sec1_bytes)
            .collect::<Result<_, _>>()
            .map_err(|_| anyhow::anyhow!("failed to parse compressed secp256k1 pubkey"))?;

        let client_state = AttestorClientState::new(attestor_addresses, min_required_sigs, height);
        let consensus_state = AttestorConsensusState { height, timestamp };

        let client_state_bz = serde_json::to_vec(&client_state)?;
        let consensus_state_bz = serde_json::to_vec(&consensus_state)?;

        let wasm_client_state = WasmClientState {
            data: client_state_bz,
            checksum,
            latest_height: Some(Height {
                revision_number: 0,
                revision_height: height,
            }),
        };
        let wasm_consensus_state = WasmConsensusState {
            data: consensus_state_bz,
        };

        let msg = MsgCreateClient {
            client_state: Some(Any::from_msg(&wasm_client_state)?),
            consensus_state: Some(Any::from_msg(&wasm_consensus_state)?),
            signer: self.signer_address.clone(),
        };

        Ok(TxBody {
            messages: vec![Any::from_msg(&msg)?],
            ..Default::default()
        }
        .encode_to_vec())
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(&self, dst_client_id: String) -> Result<Vec<u8>> {
        tracing::info!("Updating attested light client: {}", dst_client_id);
        // TODO: IBC-164
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use attestor_light_client::membership::MembershipProof;
    use attestor_packet_membership::Packets;

    #[test]
    fn abi_bytes_are_not_json() {
        let commitments = vec![[0x11u8; 32], [0x22u8; 32]];
        let abi = Packets::new(commitments).to_abi_bytes();

        let parsed: Result<Vec<Vec<u8>>, _> = serde_json::from_slice(&abi);
        assert!(
            parsed.is_err(),
            "ABI-encoded bytes32[] must not be parsed as JSON"
        );
    }

    #[test]
    fn build_membership_proof_passes_through_abi_bytes() {
        let commitments = vec![[0xAAu8; 32], [0xBBu8; 32]];
        let abi = Packets::new(commitments).to_abi_bytes();
        let signatures = vec![vec![0u8; 65], vec![1u8; 65]];

        let proof_bytes = build_membership_proof_bytes(abi.clone(), signatures.clone())
            .expect("proof should serialize");
        let proof: MembershipProof = serde_json::from_slice(&proof_bytes).expect("deserialize");

        assert_eq!(proof.attestation_data, abi);
        assert_eq!(proof.signatures, signatures);
    }
}
