//! Utilities for attested relays targeting Solana chains.

use crate::aggregator::rpc::{AggregatedAttestation, CommitmentType};
use crate::aggregator::Aggregator;
use crate::utils::attestor::get_packet_attestation;
use alloy::sol_types::SolValue;
use anyhow::Result;
use borsh::BorshSerialize;
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;
use ibc_proto_eureka::ibc::core::{
    channel::v2::{MsgAcknowledgement, MsgRecvPacket},
    client::v1::Height,
};

/// Borsh-serialized membership proof for Solana attestation light client.
///
/// Uses Borsh for efficient binary serialization (~2.5x smaller than JSON).
/// Note: Must match the format expected by the attestation light client on Solana.
#[derive(BorshSerialize)]
pub struct SolanaMembershipProof {
    /// ABI-encoded attestation data from the aggregator.
    pub attestation_data: Vec<u8>,
    /// ECDSA signatures from attestors.
    pub signatures: Vec<Vec<u8>>,
}

/// Builds a Borsh-encoded membership proof for Solana attestation light client.
///
/// # Arguments
/// * `attested_data` - ABI-encoded attestation data from the aggregator
/// * `signatures` - ECDSA signatures from attestors
/// * `max_signatures` - Maximum number of signatures to include (Solana tx size limit)
///
/// # Panics
///
/// Panics if borsh serialization of the membership proof fails.
#[must_use]
pub fn build_solana_membership_proof(
    attested_data: Vec<u8>,
    signatures: Vec<Vec<u8>>,
    max_signatures: usize,
) -> Vec<u8> {
    let limited_signatures: Vec<_> = signatures.into_iter().take(max_signatures).collect();

    let proof = SolanaMembershipProof {
        attestation_data: attested_data,
        signatures: limited_signatures,
    };

    borsh::to_vec(&proof).expect("MembershipProof serialization should not fail")
}

/// Fetches a packet attestation from the aggregator.
///
/// # Arguments
/// * `aggregator` - The aggregator client
/// * `packets` - ABI-encoded packets to get attestation for
/// * `height` - The height to fetch attestation at
/// * `commitment_type` - Type of commitment (Packet or Ack)
///
/// # Errors
/// Returns an error if fetching attestation fails.
pub async fn fetch_packet_attestation(
    aggregator: &Aggregator,
    packets: Vec<Vec<u8>>,
    height: u64,
    commitment_type: CommitmentType,
) -> Result<Option<AggregatedAttestation>> {
    if packets.is_empty() {
        return Ok(None);
    }

    let result = get_packet_attestation(aggregator, packets, height, commitment_type).await?;
    Ok(result.map(|r| r.packet))
}

/// Collects ABI-encoded packets from recv messages.
#[must_use]
pub fn collect_recv_packets(recv_msgs: &[MsgRecvPacket]) -> Vec<Vec<u8>> {
    recv_msgs
        .iter()
        .filter_map(|msg| msg.packet.as_ref())
        .map(|p| Packet::from(p.clone()).abi_encode())
        .collect()
}

/// Collects ABI-encoded packets from ack messages.
#[must_use]
pub fn collect_ack_packets(ack_msgs: &[MsgAcknowledgement]) -> Vec<Vec<u8>> {
    ack_msgs
        .iter()
        .filter_map(|msg| msg.packet.as_ref())
        .map(|p| Packet::from(p.clone()).abi_encode())
        .collect()
}

/// Injects attestation proofs into Solana-bound IBC messages.
///
/// This function:
/// 1. Collects packets from recv and ack messages
/// 2. Fetches attestations from the aggregator in parallel
/// 3. Builds Borsh-encoded proofs
/// 4. Injects proofs into the messages
///
/// # Arguments
/// * `aggregator` - The aggregator client
/// * `recv_msgs` - Mutable recv messages to inject proofs into
/// * `ack_msgs` - Mutable ack messages to inject proofs into
/// * `target_height` - Height to fetch attestations at
/// * `max_signatures` - Maximum signatures to include (Solana tx size limit)
///
/// # Errors
/// Returns an error if fetching attestations fails.
pub async fn inject_solana_attestor_proofs(
    aggregator: &Aggregator,
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    target_height: &Height,
    max_signatures: usize,
) -> Result<()> {
    let recv_packets = collect_recv_packets(recv_msgs);
    let ack_packets = collect_ack_packets(ack_msgs);

    let (recv_attestation, ack_attestation) = tokio::join!(
        fetch_packet_attestation(
            aggregator,
            recv_packets,
            target_height.revision_height,
            CommitmentType::Packet
        ),
        fetch_packet_attestation(
            aggregator,
            ack_packets,
            target_height.revision_height,
            CommitmentType::Ack
        ),
    );

    if let Some(attestation) = recv_attestation? {
        let proof_bytes = build_solana_membership_proof(
            attestation.attested_data,
            attestation.signatures,
            max_signatures,
        );
        for msg in recv_msgs.iter_mut() {
            msg.proof_commitment.clone_from(&proof_bytes);
            msg.proof_height = Some(*target_height);
        }
        tracing::info!(
            "Injected attestation proof into {} recv messages",
            recv_msgs.len()
        );
    }

    if let Some(attestation) = ack_attestation? {
        let proof_bytes = build_solana_membership_proof(
            attestation.attested_data,
            attestation.signatures,
            max_signatures,
        );
        for msg in ack_msgs.iter_mut() {
            msg.proof_acked.clone_from(&proof_bytes);
            msg.proof_height = Some(*target_height);
        }
        tracing::info!(
            "Injected attestation proof into {} ack messages",
            ack_msgs.len()
        );
    }

    Ok(())
}
