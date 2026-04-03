use anchor_lang::prelude::*;

/// Role IDs used by consuming programs (ICS26 router, IFT, GMP, etc.).
///
/// The access manager itself does not interpret these IDs — it simply stores
/// role assignments. Consuming programs define and enforce role semantics.
///
/// Commented-out IDs are not used on Solana but kept to stay consistent with
/// Ethereum-side numbering.
pub mod roles {
    pub const ADMIN_ROLE: u64 = 0;
    pub const PUBLIC_ROLE: u64 = u64::MAX;
    pub const RELAYER_ROLE: u64 = 1;
    pub const PAUSER_ROLE: u64 = 2;
    pub const UNPAUSER_ROLE: u64 = 3;
    // pub const DELEGATE_SENDER_ROLE: u64 = 4;
    // pub const RATE_LIMITER_ROLE: u64 = 5;
    pub const ID_CUSTOMIZER_ROLE: u64 = 6;
    // pub const ERC20_CUSTOMIZER_ROLE: u64 = 7;
}

/// Embedded access manager state for IBC programs.
///
/// Mirrors the on-chain struct in the access-manager program. Tracks which
/// access manager governs permissioned instructions and supports two-step
/// access manager migration (propose/accept).
///
/// Does not carry its own `_reserved` field — future fields can eat into the
/// `_reserved` space of the higher-level state that embeds this struct.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AccessManagerState {
    /// Program ID of the access manager that governs this program's roles.
    pub access_manager: Pubkey,
    /// Proposed replacement access manager, set during a pending transfer.
    pub pending_access_manager: Option<Pubkey>,
}

/// Backwards-compatible helper struct for getting access manager PDA
/// All actual types have been moved to the access-manager program
pub struct AccessManager;

impl AccessManager {
    pub const SEED: &'static [u8] = b"access_manager";

    /// Get access manager PDA (backwards compatible helper)
    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
    }
}
