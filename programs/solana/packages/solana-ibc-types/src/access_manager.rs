use anchor_lang::prelude::*;

pub mod roles {
    // Some roles are commented out, because we don't need them in Solana,
    // but for consistency with Ethereum, let's keep their ids reserved.

    pub const ADMIN_ROLE: u64 = 0;
    pub const PUBLIC_ROLE: u64 = u64::MAX;
    pub const RELAYER_ROLE: u64 = 1;
    pub const PAUSER_ROLE: u64 = 2;
    pub const UNPAUSER_ROLE: u64 = 3;
    // pub const DELEGATE_SENDER_ROLE: u64 = 4;
    // pub const RATE_LIMITER_ROLE: u64 = 5;
    pub const ID_CUSTOMIZER_ROLE: u64 = 6;
    // pub const ERC20_CUSTOMIZER_ROLE: u64 = 7;
    pub const UPGRADER_ROLE: u64 = 8;
}

/// Role data containing role ID and members
#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace, Debug)]
pub struct RoleData {
    pub role_id: u64,
    #[max_len(16)]
    pub members: Vec<Pubkey>,
}

/// Access manager state - matches the on-chain account structure in access-manager program
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, InitSpace)]
pub struct AccessManager {
    #[max_len(16)]
    pub roles: Vec<RoleData>,
}

impl AccessManager {
    pub const SEED: &'static [u8] = b"access_manager";

    /// Get access manager PDA
    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
    }
}
