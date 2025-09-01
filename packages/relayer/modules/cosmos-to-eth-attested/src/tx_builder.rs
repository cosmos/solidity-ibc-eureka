//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Ethereum from events received from an Attested chain via the aggregator.

use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use alloy::{
    network::Ethereum,
    primitives::{Address, Bytes},
    providers::Provider,
    sol_types::{SolCall, SolValue},
};
use anyhow::Result;

use ibc_eureka_relayer_lib::{
    attestations::{self, Aggregator, Config},
    chain::{Chain, CosmosSdk},
    events::{EurekaEvent, EurekaEventWithHeight},
    tx_builder::TxBuilderService,
    utils::eth_eureka,
};
use ibc_eureka_solidity_types::{
    attestor_light_client,
    ics26::{
        router::{multicallCall, routerCalls, routerInstance, updateClientCall},
        IICS02ClientMsgs::Height as ICS26Height,
        IICS26RouterMsgs::Packet,
    },
    msgs::IAttestorMsgs::AttestationProof,
};

/// The key for the minimum number of signatures
const MIN_REQUIRED_SIGS: &str = "min_required_sigs";
/// The key for the intial height for the consensus state
const HEIGHT: &str = "height";
/// The key for the initial timestamp for the consensus state
const TIMESTAMP: &str = "timestamp";
/// The key for the attestor addresses
const ATTESTOR_ADDRESSES: &str = "attestor_addresses";
/// The key for the role manager in the parameters map.
const ROLE_MANAGER: &str = "role_manager";

/// Chain type for attested chains that get their state from the aggregator
pub struct AttestedChain;

impl Chain for AttestedChain {
    type Event = EurekaEventWithHeight;
    type TxId = String;
    type Height = u64;
}

/// The `TxBuilder` produces txs to [`Ethereum`] based on attestations from the aggregator.
pub struct TxBuilder<P>
where
    P: Provider + Clone,
{
    /// The aggregator for fetching attestations.
    pub aggregator: Aggregator,
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<P, Ethereum>,
}

impl<P> TxBuilder<P>
where
    P: Provider + Clone,
{
    /// Creates a new `TxBuilder`.
    pub async fn new(
        ics26_address: Address,
        provider: P,
        aggregator_config: Config,
    ) -> Result<Self> {
        let aggregator = Aggregator::from_config(aggregator_config).await?;

        Ok(Self {
            ics26_router: routerInstance::new(ics26_address, provider),
            aggregator,
        })
    }
}

/// Build serialized membership proof bytes from ABI-encoded attested data and signatures
fn build_abi_encoded_proof(attested_data: Vec<u8>, signatures: Vec<Vec<u8>>) -> Vec<u8> {
    AttestationProof {
        attestationData: Bytes::from_iter(attested_data),
        signatures: signatures.into_iter().map(Bytes::from).collect(),
    }
    .abi_encode()
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

#[async_trait::async_trait]
impl<P> TxBuilderService<AttestedChain, CosmosSdk> for TxBuilder<P>
where
    P: Provider + Clone,
{
    #[tracing::instrument(skip(self, src_events, target_events, src_packet_seqs, dst_packet_seqs))]
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

        let msg = build_abi_encoded_proof(state.attested_data, state.signatures);
        let update_msg = routerCalls::updateClient(updateClientCall {
            clientId: dst_client_id.clone(),
            updateMsg: Bytes::from_iter(msg),
        });

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let height = ICS26Height {
            revisionNumber: 0,
            revisionHeight: query_height,
        };
        let timeout_msgs = eth_eureka::target_events_to_timeout_msgs(
            target_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &height,
            now_since_unix.as_secs(),
        );
        let mut recv_and_ack_msgs = eth_eureka::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &height,
            now_since_unix.as_secs(),
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv & ack messages: #{}", recv_and_ack_msgs.len());

        let proof = build_abi_encoded_proof(packets.attested_data, packets.signatures);
        attestations::inject_proofs_for_evm_msg(&mut recv_and_ack_msgs, &proof);

        // NOTE: UpdateMsg must come first otherwise
        // client state may not contain the needed
        // height for the RecvMsgs
        let all_calls = std::iter::once(update_msg)
            .chain(recv_and_ack_msgs)
            .chain(timeout_msgs)
            .map(|call| match call {
                routerCalls::ackPacket(call) => call.abi_encode(),
                routerCalls::recvPacket(call) => call.abi_encode(),
                routerCalls::timeoutPacket(call) => call.abi_encode(),
                _ => unreachable!("only ack, recv and timeout msgs allowed"),
            });

        let multicall_tx = multicallCall {
            data: all_calls.map(Into::into).collect(),
        };

        Ok(multicall_tx.abi_encode())
    }

    #[tracing::instrument(skip(self))]
    async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        let role_admin = parameters
            .get(ROLE_MANAGER)
            .map_or(Ok(Address::ZERO), |a| {
                Address::from_str(a.as_str()).map_err(|e| anyhow::anyhow!(e))
            })?;

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

        Ok(attestor_light_client::light_client::deploy_builder(
            self.ics26_router.provider().clone(),
            attestor_addresses,
            min_required_sigs,
            height,
            timestamp,
            role_admin,
        )
        .calldata()
        .to_vec())
    }

    #[tracing::instrument(skip(self))]
    async fn update_client(&self, _dst_client_id: String) -> Result<Vec<u8>> {
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
