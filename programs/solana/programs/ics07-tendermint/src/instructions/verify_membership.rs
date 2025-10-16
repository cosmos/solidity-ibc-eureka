use crate::error::ErrorCode;
use crate::helpers::{deserialize_merkle_proof, validate_proof_params};
use crate::VerifyMembership;
use anchor_lang::prelude::*;
use ics25_handler::MembershipMsg;
use tendermint_light_client_membership::KVPair;

pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
    require!(!msg.value.is_empty(), ErrorCode::MembershipEmptyValue);

    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    msg!("=== VERIFY MEMBERSHIP START ===");
    msg!("  Height: {}", msg.height);
    msg!("  Path segments: {}", msg.path.len());
    msg!("  Value length: {} bytes", msg.value.len());
    msg!("  Full value (hex): {:?}", &msg.value);
    msg!("  Proof length: {} bytes", msg.proof.len());
    msg!(
        "  Proof (first 128 bytes): {:?}",
        &msg.proof[..msg.proof.len().min(128)]
    );

    msg!("=== PATH COMPONENTS ===");
    for (idx, segment) in msg.path.iter().enumerate() {
        msg!("  Path[{}] length: {} bytes", idx, segment.len());
        msg!("  Path[{}] (hex): {:?}", idx, segment);
    }

    msg!("=== CONSENSUS STATE ===");
    msg!("  Consensus state height: {}", msg.height);
    msg!(
        "  Full app_hash from consensus state: {:?}",
        consensus_state_store.consensus_state.root
    );
    msg!(
        "  App hash length: {} bytes",
        consensus_state_store.consensus_state.root.len()
    );
    msg!(
        "  Consensus state timestamp: {}",
        consensus_state_store.consensus_state.timestamp
    );

    validate_proof_params(client_state, &msg)?;

    let proof = deserialize_merkle_proof(&msg.proof)?;
    msg!("  Proof deserialized successfully");
    msg!("  Proof specs count: {}", proof.proofs.len());

    let kv_pair = KVPair::new(msg.path.clone(), msg.value.clone());
    msg!("=== MEMBERSHIP VERIFICATION ===");
    msg!(
        "  Verifying path against app_hash: {:?}",
        consensus_state_store.consensus_state.root
    );
    msg!("  Path[0] (hex): {:?}", &msg.path[0]);
    if msg.path.len() > 1 {
        msg!("  Path[1] (hex): {:?}", &msg.path[1]);
    }

    let app_hash = consensus_state_store.consensus_state.root;

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)]).map_err(|e| {
        msg!("=== MEMBERSHIP VERIFICATION FAILED ===");
        msg!("  Error: {:?}", e);
        msg!("  Expected app_hash: {:?}", app_hash);
        msg!(
            "  Value being verified: {:?}",
            &msg.value[..msg.value.len().min(32)]
        );
        error!(ErrorCode::MembershipVerificationFailed)
    })?;

    msg!("=== MEMBERSHIP VERIFICATION SUCCEEDED ===");
    Ok(())
}
