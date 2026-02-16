use anchor_lang::prelude::*;

/// On-chain PDA storing the consensus state for a specific block height.
/// The height is also encoded in the PDA seeds.
#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    /// Block height this consensus state corresponds to.
    pub height: u64,
    /// Unix timestamp in seconds for this block height.
    pub timestamp: u64,
}

impl ConsensusStateStore {
    pub const SEED: &'static [u8] = b"consensus_state";

    pub fn pda(height: u64) -> Pubkey {
        Pubkey::find_program_address(&[Self::SEED, &height.to_le_bytes()], &crate::ID).0
    }
}
