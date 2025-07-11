use crate::types::ConsensusState;
use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}
