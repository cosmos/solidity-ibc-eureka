use crate::error::ErrorCode;
use crate::helpers::{keccak256, sha256, verify_signatures_threshold, AttestationProof};
use alloy_sol_types::SolType;
use anchor_lang::prelude::*;
use ibc_eureka_solidity_types::msgs::IAttestationMsgs;
use ics25_handler::MembershipMsg;

pub fn handler(ctx: Context<crate::VerifyMembership>, msg: MembershipMsg) -> Result<u64> {
    let client_state = &ctx.accounts.client_state;
    let consensus_state = &ctx.accounts.consensus_state;

    // Check if client is frozen
    require!(!client_state.is_frozen(), ErrorCode::ClientFrozen);

    // Validate value is not empty
    require!(!msg.value.is_empty(), ErrorCode::EmptyValue);

    // Validate path length
    require!(msg.path.len() == 1, ErrorCode::InvalidPathLength);

    // Deserialize proof
    // TODO: From where are we getting the proof. In what format is it? Is there an existing type we can use?
    let proof: AttestationProof =
        serde_json::from_slice(&msg.proof).map_err(|_| ErrorCode::DeserializationFailed)?;

    // Validate attestor signatures
    let digest = sha256(&proof.attestation_data);
    verify_signatures_threshold(
        digest,
        &proof.signatures,
        ctx.accounts.client_state.attestor_addresses.as_slice(),
        ctx.accounts.client_state.min_required_sigs,
    )?;

    // Decode PacketAttestation from ABI-encoded attestation_data
    let packet_attestation =
        IAttestationMsgs::PacketAttestation::abi_decode(&proof.attestation_data)
            .map_err(|_| ErrorCode::AbiDecodingFailed)?;

    // Verify packet attestation height matches
    require!(
        packet_attestation.height == consensus_state.height,
        ErrorCode::HeightMismatch
    );

    // Ensure packets list is not empty
    require!(
        !packet_attestation.packets.is_empty(),
        ErrorCode::EmptyPackets
    );

    // Compute path hash (keccak256 of the path)
    let path_hash = keccak256(&msg.path[0]);

    // Search for the packet in the attested list
    let packet = packet_attestation
        .packets
        .iter()
        .find(|p| p.path == path_hash)
        .ok_or(ErrorCode::PathNotFound)?;

    // Compute commitment hash from value
    // TODO: This is the way?
    let value_hash = keccak256(&msg.value);

    // Verify commitment matches
    require!(packet.commitment == value_hash, ErrorCode::NotMember);

    msg!(
        "Membership verified: height={}, path_hash={:?}, timestamp={}",
        consensus_state.height,
        path_hash,
        consensus_state.timestamp
    );

    Ok(consensus_state.timestamp)
}
