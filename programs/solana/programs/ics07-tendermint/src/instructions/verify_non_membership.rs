use crate::error::ErrorCode;
use crate::helpers::{deserialize_merkle_proof, validate_proof_params};
use crate::VerifyNonMembership;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;
use ics25_handler::MembershipMsg;
use tendermint_light_client_membership::KVPair;

pub fn verify_non_membership(ctx: Context<VerifyNonMembership>, msg: MembershipMsg) -> Result<()> {
    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    validate_proof_params(client_state, &msg)?;

    // For non-membership, the value must be empty
    require!(msg.value.is_empty(), ErrorCode::InvalidValue);

    let proof = deserialize_merkle_proof(&msg.proof)?;
    let kv_pair = KVPair::new(msg.path, vec![]);
    let app_hash = consensus_state_store.consensus_state.root;

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)]).map_err(|e| {
        msg!("Non-membership verification failed: {:?}", e);
        error!(ErrorCode::NonMembershipVerificationFailed)
    })?;

    // Return the consensus state timestamp for timeout verification
    let timestamp_bytes = consensus_state_store
        .consensus_state
        .timestamp
        .to_le_bytes();
    set_return_data(&timestamp_bytes);

    Ok(())
}
