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

    validate_proof_params(client_state, consensus_state_store, &msg)?;

    let proof = deserialize_merkle_proof(&msg.proof)?;
    let kv_pair = KVPair::new(msg.path.clone(), msg.value.clone());
    let app_hash = consensus_state_store.consensus_state.root;

    // Log detailed info for debugging the mismatch
    msg!("=== MEMBERSHIP VERIFICATION DEBUG ===");
    msg!("Height: {}", msg.height);
    msg!("App hash: {}", hex::encode(&app_hash));
    msg!("Path count: {}", msg.path.len());

    // Build the full path for IBC commitment
    let mut full_path = Vec::new();
    for segment in &msg.path {
        full_path.extend_from_slice(segment);
        if segment != msg.path.last().unwrap() {
            full_path.push(b'/');
        }
    }
    msg!("Full path: {}", String::from_utf8_lossy(&full_path));

    msg!("Value (hex): {}", hex::encode(&msg.value));
    msg!("Proof len: {} bytes", msg.proof.len());

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)]).map_err(|e| {
        msg!("Verification failed: {:?}", e);
        error!(ErrorCode::MembershipVerificationFailed)
    })?;

    Ok(())
}
