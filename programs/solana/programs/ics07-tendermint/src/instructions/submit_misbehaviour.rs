use anchor_lang::prelude::*;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use tendermint_light_client_update_client::ClientState as TmClientState;
use crate::error::ErrorCode;
use crate::helpers::deserialize_misbehaviour;
use crate::types::MisbehaviourMsg;
use crate::SubmitMisbehaviour;

pub fn submit_misbehaviour(
    ctx: Context<SubmitMisbehaviour>,
    msg: MisbehaviourMsg,
) -> Result<()> {
    let client_data = &mut ctx.accounts.client_data;

    require!(!client_data.frozen, ErrorCode::ClientAlreadyFrozen);

    let misbehaviour = deserialize_misbehaviour(&msg.misbehaviour)?;
    let client_state: TmClientState = client_data.client_state.clone().into();

    let trusted_consensus_state_1: IbcConsensusState = ctx
        .accounts
        .trusted_consensus_state_1
        .consensus_state
        .clone()
        .into();
    let trusted_consensus_state_2: IbcConsensusState = ctx
        .accounts
        .trusted_consensus_state_2
        .consensus_state
        .clone()
        .into();

    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    let output = tendermint_light_client_misbehaviour::check_for_misbehaviour(
        &client_state,
        &misbehaviour,
        trusted_consensus_state_1,
        trusted_consensus_state_2,
        current_time,
    )
    .map_err(|e| {
        msg!("Misbehaviour check failed: {:?}", e);
        error!(ErrorCode::MisbehaviourFailed)
    })?;

    require!(
        ctx.accounts.trusted_consensus_state_1.height
            == output.trusted_height_1.revision_height(),
        ErrorCode::InvalidHeight
    );
    require!(
        ctx.accounts.trusted_consensus_state_2.height
            == output.trusted_height_2.revision_height(),
        ErrorCode::InvalidHeight
    );

    // If we reach here, misbehaviour was detected
    // Freeze the client at the current height
    client_data.frozen = true;
    client_data.client_state.frozen_height = client_data.client_state.latest_height;

    msg!(
        "Misbehaviour detected at heights {:?} and {:?}",
        output.trusted_height_1,
        output.trusted_height_2
    );

    Ok(())
}