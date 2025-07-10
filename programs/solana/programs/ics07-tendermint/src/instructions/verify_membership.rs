use anchor_lang::prelude::*;
use tendermint_light_client_membership::KVPair;
use crate::error::ErrorCode;
use crate::helpers::{deserialize_merkle_proof, validate_proof_params};
use crate::types::MembershipMsg;
use crate::VerifyMembership;

pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
    let client_data = &ctx.accounts.client_data;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    validate_proof_params(client_data, consensus_state_store, &msg)?;

    let proof = deserialize_merkle_proof(&msg.proof)?;
    let kv_pair = KVPair::new(vec![msg.path.clone()], msg.value);
    let app_hash = consensus_state_store.consensus_state.root;

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)]).map_err(
        |e| {
            msg!("Membership verification failed: {:?}", e);
            error!(ErrorCode::MembershipVerificationFailed)
        },
    )?;

    Ok(())
}