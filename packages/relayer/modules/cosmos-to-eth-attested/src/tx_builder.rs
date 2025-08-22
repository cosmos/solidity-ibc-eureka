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
use ibc_proto_eureka::{
    google::protobuf::Any,
    ibc::{
        core::client::v1::{Height, MsgCreateClient},
        lightclients::wasm::v1::{
            ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        },
    },
};
use prost::Message;
use tonic::transport::Channel;

use ibc_eureka_relayer_lib::{
    chain::{Chain, CosmosSdk},
    events::{EurekaEvent, EurekaEventWithHeight},
    tx_builder::TxBuilderService,
    utils::{attestor, eth_eureka},
};
use ibc_eureka_solidity_types::ics26::{
    router::{multicallCall, routerCalls, updateClientCall},
    IICS02ClientMsgs::Height as ICS20Height,
    IICS26RouterMsgs::Packet,
};

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
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub fn new(aggregator_url: String) -> Self {
        Self { aggregator_url }
    }

    /// Creates an aggregator client.
    async fn create_aggregator_client(&self) -> Result<AggregatorClient> {
        let channel = Channel::from_shared(self.aggregator_url.clone())?
            .connect()
            .await?;
        Ok(AggregatorClient::new(channel))
    }
}

/// Build serialized membership proof bytes from ABI-encoded attested data and signatures
fn build_membership_proof_bytes(
    attested_data: Vec<u8>,
    signatures: Vec<Vec<u8>>,
) -> anyhow::Result<Vec<u8>> {
    let structured = MembershipProof {
        attestation_data: attested_data,
        signatures,
    };
    serde_json::to_vec(&structured).map_err(|_| anyhow::anyhow!("proof could not be serialized"))
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
const ATTESTOR_ADDRESSES: &str = "attestor_addresses";
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
        );
        let header_bz = serde_json::to_vec(&header)
            .map_err(|_| anyhow::anyhow!("header could not be serialized"))?;

        let update_msg = routerCalls::updateClient(updateClientCall {
            clientId: dst_client_id.clone(),
            // TODO: Use solidity msg type
            updateMsg: alloy::primitives::Bytes::from_iter(header_bz),
        });

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let timeout_msgs = eth_eureka::target_events_to_timeout_msgs(
            target_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &ICS20Height {
                revisionHeight: query_height,
                revisionNumber: 0,
            },
            now_since_unix.as_secs(),
        );
        let mut recv_and_ack_msgs = eth_eureka::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &ICS20Height {
                revisionHeight: query_height,
                // The attestor does not care about this
                revisionNumber: 0,
            },
            now_since_unix.as_secs(),
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv & ack messages: #{}", recv_and_ack_msgs.len());

        let proof = build_membership_proof_bytes(packets.attested_data, packets.signatures)?;
        attestor::inject_proofs(&mut recv_msgs, &proof, packets.height);

        // NOTE: UpdateMsg must come first otherwise
        // client state may not contain the needed
        // height for the RecvMsgs
        let all_calls = std::iter::once(update_msg)
            .chain(recv_and_ack_msgs)
            .chain(timeout_msgs)
            .into_iter()
            .map(|call| match call {
                routerCalls::ackPacket(call) => call.abi_encode(),
                routerCalls::recvPacket(call) => call.abi_encode(),
                routerCalls::timeoutPacket(call) => call.abi_encode(),
                _ => unreachable!("only ack, recv msg and timeout msgs allowed"),
            });

        let multicall_tx = multicallCall {
            data: all_calls.map(Into::into).collect(),
        };

        Ok(multicall_tx.abi_encode())
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

        let addrs_hex = parameters
            .get(ATTESTOR_ADDRESSES)
            .ok_or_else(|| anyhow::anyhow!(format!("Missing `{ATTESTOR_ADDRESSES}` parameter")))?;
        // Accept comma- or space-separated list of 0x addresses
        let attestor_addresses: Vec<Address> = addrs_hex
            .split([',', ' '])
            .filter(|s| !s.is_empty())
            .map(|s| Address::parse_checksummed(s, None))
            .collect::<Result<_, _>>()
            .map_err(|_| anyhow::anyhow!("failed to parse ethereum address list"))?;

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

        let msg = &MsgCreateClient {
            client_state: Some(Any::from_msg(&wasm_client_state)?),
            consensus_state: Some(Any::from_msg(&wasm_consensus_state)?),
            signer: "TODO".into(),
        }
        .encode_to_vec();

        let msg = updateClientCall {
            clientId: "TODO".into(),
            updateMsg: Bytes::from_iter(msg),
        }
        .abi_encode();

        Ok(multicallCall {
            data: vec![Bytes::from_iter(msg)],
        }
        .abi_encode())
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
    use attestor_packet_membership::PacketCommitments;

    #[test]
    fn abi_bytes_are_not_json() {
        let commitments = vec![[0x11u8; 32], [0x22u8; 32]];
        let abi = PacketCommitments::new(commitments).to_abi_bytes();

        let parsed: Result<Vec<Vec<u8>>, _> = serde_json::from_slice(&abi);
        assert!(
            parsed.is_err(),
            "ABI-encoded bytes32[] must not be parsed as JSON"
        );
    }

    #[test]
    fn build_membership_proof_passes_through_abi_bytes() {
        let commitments = vec![[0xAAu8; 32], [0xBBu8; 32]];
        let abi = PacketCommitments::new(commitments).to_abi_bytes();
        let signatures = vec![vec![0u8; 65], vec![1u8; 65]];

        let proof_bytes = build_membership_proof_bytes(abi.clone(), signatures.clone())
            .expect("proof should serialize");
        let proof: MembershipProof = serde_json::from_slice(&proof_bytes).expect("deserialize");

        assert_eq!(proof.attestation_data, abi);
        assert_eq!(proof.signatures, signatures);
    }
}
