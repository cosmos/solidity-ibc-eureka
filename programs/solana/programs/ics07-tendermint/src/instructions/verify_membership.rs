use crate::error::ErrorCode;
use crate::helpers::{deserialize_merkle_proof, validate_proof_params};
use crate::VerifyMembership;
use anchor_lang::prelude::*;
use hex;
use ics25_handler::MembershipMsg;
use tendermint_light_client_membership::KVPair;

pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
    require!(!msg.value.is_empty(), ErrorCode::MembershipEmptyValue);

    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    validate_proof_params(client_state, &msg)?;

    let proof = deserialize_merkle_proof(&msg.proof)?;
    let kv_pair = KVPair::new(msg.path.clone(), msg.value.clone());
    let app_hash = consensus_state_store.consensus_state.root;

    // mismatch
    msg!("=== MEMBERSHIP VERIFICATION DEBUG ===");
    msg!("Height: {}", msg.height);
    msg!("App hash: {}", hex::encode(&app_hash));
    msg!("Path count: {}", msg.path.len());

    // path segments
    if msg.path.len() > 0 {
        msg!("Path segment 0: {}", String::from_utf8_lossy(&msg.path[0]));
    }
    if msg.path.len() > 1 {
        msg!("Path segment 1 (hex): {}", hex::encode(&msg.path[1]));
    }

    msg!("Value (hex): {}", hex::encode(&msg.value));
    msg!("Proof len: {} bytes", msg.proof.len());

    // before
    msg!("=== PROOF VERIFICATION DETAILS ===");
    msg!("KVPair path segments: {}", msg.path.len());
    for (i, segment) in msg.path.iter().enumerate() {
        msg!(
            "  Path[{}]: {} (hex: {})",
            i,
            String::from_utf8_lossy(segment),
            hex::encode(segment)
        );
    }
    msg!("KVPair value len: {}", msg.value.len());
    msg!("Proof bytes len: {}", msg.proof.len());

    // Verify that the proof can be deserialized
    msg!("Deserializing proof...");
    let deserialized_proof = proof.clone();
    msg!("Proof deserialized successfully");

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)]).map_err(|e| {
        msg!("Verification failed: {:?}", e);
        msg!("This typically means:");
        msg!("  - The proof doesn't match the commitment root (app hash)");
        msg!("  - The path encoding is incorrect");
        msg!("  - The value doesn't match what was proven");
        error!(ErrorCode::MembershipVerificationFailed)
    })?;

    Ok(())
}
