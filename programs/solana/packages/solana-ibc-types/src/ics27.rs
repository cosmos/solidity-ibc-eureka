//! ICS27 GMP (General Message Passing) types for PDA derivation
//!
//! These marker types are shared between the ICS27 GMP program and relayer
//! to ensure consistent PDA derivation across the system.

use anchor_lang::prelude::*;
use solana_program::hash::hash;

/// Marker type for GMP account state PDA
pub struct GmpAccountState;

impl GmpAccountState {
    /// Seed for individual account state PDAs in the GMP program
    /// Used by both the GMP program and relayer for deriving GMP account addresses
    pub const SEED: &'static [u8] = b"gmp_account";

    /// Get GMP account state PDA for a sender
    ///
    /// This matches the PDA derivation in the ICS27 GMP program's `AccountState::derive_address`.
    /// The sender is always hashed to ensure consistent PDA derivation regardless of address length.
    pub fn pda(client_id: &str, sender: &str, salt: &[u8], program_id: Pubkey) -> (Pubkey, u8) {
        let sender_hash = hash(sender.as_bytes()).to_bytes();
        Pubkey::find_program_address(
            &[Self::SEED, client_id.as_bytes(), &sender_hash, salt],
            &program_id,
        )
    }
}

/// Marker type for GMP application state PDA
pub struct GMPAppState;

impl GMPAppState {
    /// Seed for the main GMP application state PDA
    /// Follows the standard IBC app pattern: [`APP_STATE_SEED`, `port_id`]
    pub const SEED: &'static [u8] = b"app_state";
}
