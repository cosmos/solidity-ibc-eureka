use anchor_lang::prelude::*;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use tendermint_light_client_update_client::ClientState as UpdateClientState;
use crate::error::ErrorCode;
use crate::helpers::deserialize_header;
use crate::types::{ConsensusState, UpdateClientMsg};
use crate::UpdateClient;

pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<()> {
    let client_data = &mut ctx.accounts.client_data;

    require!(!client_data.frozen, ErrorCode::ClientFrozen);

    let header = deserialize_header(&msg.client_message)?;

    let client_state: UpdateClientState = client_data.client_state.clone().into();
    let trusted_consensus_state: IbcConsensusState = client_data.consensus_state.clone().into();

    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    let output = tendermint_light_client_update_client::update_client(
        &client_state,
        &trusted_consensus_state,
        header,
        current_time,
    )
    .map_err(|e| {
        msg!("Update client failed: {:?}", e);
        error!(ErrorCode::UpdateClientFailed)
    })?;

    client_data.client_state.latest_height = output.latest_height.revision_height();
    let new_consensus_state: ConsensusState = output.new_consensus_state.clone().into();
    client_data.consensus_state = new_consensus_state.clone();

    let consensus_state_store = &mut ctx.accounts.consensus_state_store;
    consensus_state_store.height = output.latest_height.revision_height();
    consensus_state_store.consensus_state = new_consensus_state;

    Ok(())
}