use crate::error::ErrorCode;
use crate::helpers::{decode_packet_attestation, keccak256, AttestationProof};
use anchor_lang::prelude::*;
use ics25_handler::NonMembershipMsg;

pub fn handler(ctx: Context<crate::VerifyNonMembership>, msg: NonMembershipMsg) -> Result<()> {
    let client_state = &ctx.accounts.client_state;
    let consensus_state = &ctx.accounts.consensus_state;

    // Check if client is frozen
    require!(!client_state.is_frozen(), ErrorCode::ClientFrozen);

    // TODO: CRITICAL - Add signature verification here!
    // Before processing the proof, we MUST verify the attestor signatures.
    // The Solidity implementation does this at line 180-181:
    //   bytes32 digest = sha256(proof.attestationData);
    //   _verifySignaturesThreshold(digest, proof.signatures);
    //
    // Required steps:
    // 1. Parse proof JSON to extract attestation_data and signatures
    // 2. Compute SHA256 digest of attestation_data
    // 3. Call verify_signatures_threshold(digest, signatures, client_state.attestor_addresses, client_state.min_required_sigs)
    // 4. Only proceed with non-membership verification if signatures are valid
    //
    // WITHOUT this verification, anyone can submit fake proofs and the light client
    // will accept them as valid! This is a CRITICAL security vulnerability.
    // See: contracts/light-clients/attestation/AttestationLightClient.sol:179-181

    // Validate path length
    require!(msg.path.len() == 1, ErrorCode::InvalidPathLength);

    // Deserialize proof from JSON
    let proof: AttestationProof =
        serde_json::from_slice(&msg.proof).map_err(|_| ErrorCode::JsonDeserializationFailed)?;

    // Decode PacketAttestation from ABI-encoded attestation_data
    let packet_attestation = decode_packet_attestation(&proof.attestation_data)?;

    // Verify height matches
    require!(
        packet_attestation.height == consensus_state.height,
        ErrorCode::HeightMismatch
    );

    // Ensure packets list is not empty
    require!(!packet_attestation.packets.is_empty(), ErrorCode::EmptyPackets);

    // Compute path hash (keccak256 of the path)
    let path_hash = keccak256(&msg.path[0]);

    // Search for the packet in the attested list
    let packet = packet_attestation
        .packets
        .iter()
        .find(|p| p.path == path_hash)
        .ok_or(ErrorCode::PathNotFound)?;

    // Verify commitment is zero (non-membership proof)
    require!(
        packet.commitment == [0u8; 32],
        ErrorCode::CommitmentNotZero
    );

    msg!(
        "Non-membership verified: height={}, path_hash={:?}",
        consensus_state.height,
        path_hash
    );

    Ok(())
}
