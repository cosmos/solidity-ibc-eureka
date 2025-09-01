//! This mod
//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from an Attested chain via the aggregator.

use std::collections::{HashMap, HashSet};

use alloy::{primitives::Address, sol_types::SolValue};
use anyhow::Result;
use attestor_light_client::{
    client_state::ClientState as AttestorClientState,
    consensus_state::ConsensusState as AttestorConsensusState, header::Header,
    membership::MembershipProof,
};
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
use prost::Message;
use tendermint_rpc::HttpClient;

use ibc_eureka_relayer_lib::{
    aggregator::{Aggregator, Config},
    chain::{Chain, CosmosSdk},
    events::{EurekaEvent, EurekaEventWithHeight},
    tx_builder::TxBuilderService,
    utils::{attested, cosmos},
};
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;

/// Chain type for attested chains that get their state from the aggregator
pub struct AttestedChain;

impl Chain for AttestedChain {
    type Event = EurekaEventWithHeight;
    type TxId = String;
    type Height = u64;
}

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on attestations from the aggregator.
pub struct TxBuilder {
    /// The aggregator URL for fetching attestations.
    pub aggregator: Aggregator,
    /// The HTTP client for the target chain.
    pub target_tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    pub async fn new(
        aggregator_config: Config,
        target_tm_client: HttpClient,
        signer_address: String,
    ) -> Result<Self> {
        let aggregator = Aggregator::from_config(aggregator_config.clone()).await?;

        Ok(Self {
            aggregator,
            target_tm_client,
            signer_address,
        })
    }
}

/// Build serialized membership proof bytes from ABI-encoded attested data and signatures
fn build_membership_proof(
    attested_data: Vec<u8>,
    signatures: Vec<Vec<u8>>,
) -> Result<Vec<u8>, anyhow::Error> {
    serde_json::to_vec(&MembershipProof {
        attestation_data: attested_data,
        signatures,
    })
    .map_err(Into::into)
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

        tracing::info!(
            "Requesting state attestation from aggregator for {} packets",
            ics26_send_packets.len()
        );

        let (state, packets) = self
            .aggregator
            .get_attestations(ics26_send_packets, query_height)
            .await?;

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

        let proof = build_membership_proof(packets.attested_data, packets.signatures)?;
        attested::inject_proofs_for_tm_msg(&mut recv_msgs, &proof, packets.height);

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

        let addrs_hex = parameters
            .get(ATTESTOR_ADDRESSES)
            .ok_or_else(|| anyhow::anyhow!(format!("Missing `{ATTESTOR_ADDRESSES}` parameter")))?;
        // Accept comma- or space-separated list of 0x addresses
        let attestor_addresses: Vec<Address> = addrs_hex
            .split(&[',', ' '][..])
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
}
