use anchor_lang::prelude::*;
use crate::types::{ClientState, ConsensusState};

#[account]
#[derive(InitSpace)]
pub struct ClientData {
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
    pub frozen: bool,
}

#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}
