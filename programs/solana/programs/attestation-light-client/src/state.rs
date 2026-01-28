use crate::types::ConsensusState;
use anchor_lang::prelude::*;

/// Storage for consensus state at a specific height
#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}

impl ConsensusStateStore {
    pub const SEED: &'static [u8] = b"consensus_state";

    pub fn pda(client_state_key: &Pubkey, height: u64) -> Pubkey {
        Pubkey::find_program_address(
            &[Self::SEED, client_state_key.as_ref(), &height.to_le_bytes()],
            &crate::ID,
        )
        .0
    }
}
