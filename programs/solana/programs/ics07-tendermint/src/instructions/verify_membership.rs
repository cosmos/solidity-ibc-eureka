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

    msg!(
        "Verifying membership at height: {}, path length: {}, value length: {}",
        msg.height,
        msg.path.len(),
        msg.value.len()
    );

    msg!(
        "App hash from consensus state: {:?}",
        consensus_state_store.consensus_state.root
    );

    validate_proof_params(client_state, &msg)?;

    let proof = deserialize_merkle_proof(&msg.proof)?;
    msg!("Proof deserialized successfully");

    let kv_pair = KVPair::new(msg.path.clone(), msg.value.clone());
    msg!(
        "KV pair created - key length: {}, value length: {}",
        msg.path.len(),
        msg.value.len()
    );

    let app_hash = consensus_state_store.consensus_state.root;
    msg!("Starting Tendermint membership verification...");

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)])
        .map_err(|e| {
            msg!("Membership verification failed with error: {:?}", e);
            error!(ErrorCode::MembershipVerificationFailed)
        })?;

    msg!("Membership verification succeeded!");
    Ok(())
}
