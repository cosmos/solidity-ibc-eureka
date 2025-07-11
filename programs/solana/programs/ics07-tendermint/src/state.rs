use anchor_lang::prelude::*;
use crate::types::ConsensusState;

#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}
