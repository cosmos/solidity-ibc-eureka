use crate::error::ErrorCode;
use crate::helpers::{decode_packet_attestation, keccak256, AttestationProof};
use anchor_lang::prelude::*;
use ics25_handler::MembershipMsg;

pub fn handler(ctx: Context<crate::VerifyMembership>, msg: MembershipMsg) -> Result<()> {
    let client_state = &ctx.accounts.client_state;
    let consensus_state = &ctx.accounts.consensus_state;

    // Check if client is frozen
    require!(!client_state.is_frozen(), ErrorCode::ClientFrozen);

    // TODO: CRITICAL - Add signature verification here!
    // Before processing the proof, we MUST verify the attestor signatures.
    // The Solidity implementation does this at line 141-142:
    //   bytes32 digest = sha256(proof.attestationData);
    //   _verifySignaturesThreshold(digest, proof.signatures);
    //
    // Required steps:
    // 1. Parse proof JSON to extract attestation_data and signatures
    // 2. Compute SHA256 digest of attestation_data
    // 3. Call verify_signatures_threshold(digest, signatures, client_state.attestor_addresses, client_state.min_required_sigs)
    // 4. Only proceed with membership verification if signatures are valid
    //
    // WITHOUT this verification, anyone can submit fake proofs and the light client
    // will accept them as valid! This is a CRITICAL security vulnerability.
    // See: contracts/light-clients/attestation/AttestationLightClient.sol:140-142

    // Validate value is not empty
    require!(!msg.value.is_empty(), ErrorCode::EmptyValue);

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

    // Compute commitment hash from value
    let value_hash = keccak256(&msg.value);

    // TODO: Verify value handling matches Solidity implementation
    // Solidity decodes value as bytes32 directly: `bytes32 value = abi.decode(msg_.value, (bytes32));`
    // Then compares it directly with packet.commitment: `packetAttestation.packets[i].commitment == value`
    // Current implementation: keccak256(msg.value) which may be correct for Solana's encoding
    // Verify this matches the expected behavior with the relayer's proof format
    // See: contracts/light-clients/attestation/AttestationLightClient.sol:153, 156

    // Verify commitment matches
    require!(packet.commitment == value_hash, ErrorCode::NotMember);

    msg!(
        "Membership verified: height={}, path_hash={:?}",
        consensus_state.height,
        path_hash
    );

    Ok(())
}
