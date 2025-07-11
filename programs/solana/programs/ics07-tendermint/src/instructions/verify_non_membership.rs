use crate::error::ErrorCode;
use crate::helpers::{deserialize_merkle_proof, validate_proof_params};
use crate::types::MembershipMsg;
use crate::VerifyNonMembership;
use anchor_lang::prelude::*;
use tendermint_light_client_membership::KVPair;

pub fn verify_non_membership(ctx: Context<VerifyNonMembership>, msg: MembershipMsg) -> Result<()> {
    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    validate_proof_params(client_state, consensus_state_store, &msg)?;

    // For non-membership, the value must be empty
    require!(msg.value.is_empty(), ErrorCode::InvalidValue);

    let proof = deserialize_merkle_proof(&msg.proof)?;
    let kv_pair = KVPair::new(msg.path, vec![]);
    let app_hash = consensus_state_store.consensus_state.root;

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)]).map_err(|e| {
        msg!("Non-membership verification failed: {:?}", e);
        error!(ErrorCode::NonMembershipVerificationFailed)
    })?;

    Ok(())
}
