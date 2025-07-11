use anchor_lang::prelude::*;
use crate::error::ErrorCode;
use crate::types::{ClientState, ConsensusState};
use crate::Initialize;

pub fn initialize(
    ctx: Context<Initialize>,
    client_state: ClientState,
    consensus_state: ConsensusState,
) -> Result<()> {
    require!(!client_state.chain_id.is_empty(), ErrorCode::InvalidChainId);

    require!(
        client_state.trust_level_numerator > 0
            && client_state.trust_level_numerator <= client_state.trust_level_denominator
            && client_state.trust_level_denominator > 0,
        ErrorCode::InvalidTrustLevel
    );

    require!(
        client_state.trusting_period > 0
            && client_state.unbonding_period > 0
            && client_state.trusting_period < client_state.unbonding_period,
        ErrorCode::InvalidPeriods
    );

    require!(
        client_state.max_clock_drift > 0,
        ErrorCode::InvalidMaxClockDrift
    );

    require!(client_state.latest_height.revision_height > 0, ErrorCode::InvalidHeight);

    let client_data = &mut ctx.accounts.client_data;
    client_data.client_state = client_state.clone();
    client_data.frozen = false;

    let consensus_state_store = &mut ctx.accounts.consensus_state_store;
    consensus_state_store.height = client_state.latest_height.revision_height;
    consensus_state_store.consensus_state = consensus_state;

    Ok(())
}
