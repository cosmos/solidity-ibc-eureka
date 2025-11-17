//! ICS27 GMP (General Message Passing) types for PDA derivation
//!
//! These types are shared between the ICS27 GMP program and relayer
//! to ensure consistent PDA derivation across the system.

use anchor_lang::prelude::*;
use solana_program::hash::hash;

// Re-export from solana-ibc-proto
pub use solana_ibc_proto::{
    ClientId, ConstrainedBytes, ConstrainedError, ConstrainedString, ConstrainedVec,
    GMPPacketError, Memo, Receiver, Salt, Sender, ValidatedGmpPacketData,
};

/// GMP account identifier for PDA derivation
///
/// This type provides stateless PDA derivation for cross-chain account abstraction.
/// Each unique combination of (client_id, sender, salt) derives a unique GMP account PDA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GMPAccount {
    pub client_id: ClientId,
    pub sender: Sender,
    pub salt: Salt,
    pub sender_hash: [u8; 32],
    pub pda: Pubkey,
    pub bump: u8,
}

impl GMPAccount {
    /// Seed for individual account PDAs in the GMP program
    pub const SEED: &'static [u8] = b"gmp_account";

    /// Create a new GMPAccount with PDA derivation
    ///
    /// Accepts validated types, so no validation needed - construction cannot fail
    pub fn new(client_id: ClientId, sender: Sender, salt: Salt, program_id: &Pubkey) -> Self {
        // Calculate hash and PDA
        let sender_hash = hash(sender.as_bytes()).to_bytes();
        let (pda, bump) = Pubkey::find_program_address(
            &[Self::SEED, client_id.as_bytes(), &sender_hash, &salt],
            program_id,
        );

        Self {
            client_id,
            sender,
            salt,
            sender_hash,
            pda,
            bump,
        }
    }

    /// Get the derived PDA and bump
    pub fn pda(&self) -> (Pubkey, u8) {
        (self.pda, self.bump)
    }

    /// Create signer seeds for use with invoke_signed
    pub fn to_signer_seeds(&self) -> SignerSeeds {
        SignerSeeds {
            client_id: self.client_id.clone(),
            sender_hash: self.sender_hash,
            salt: self.salt.clone(),
            bump: self.bump,
        }
    }

    /// Invoke a cross-program instruction with this GMP account as signer
    pub fn invoke_signed(
        &self,
        instruction: &anchor_lang::solana_program::instruction::Instruction,
        account_infos: &[anchor_lang::prelude::AccountInfo],
    ) -> Result<()> {
        let seeds = self.to_signer_seeds();
        let seeds_slices = seeds.as_slices();
        anchor_lang::solana_program::program::invoke_signed(
            instruction,
            account_infos,
            &[&seeds_slices],
        )
        .map_err(|e| e.into())
    }
}

/// Signer seeds wrapper for invoke_signed
pub struct SignerSeeds {
    client_id: ClientId,
    sender_hash: [u8; 32],
    salt: Salt,
    bump: u8,
}

impl SignerSeeds {
    /// Get seeds as slices for invoke_signed
    pub fn as_slices(&self) -> [&[u8]; 5] {
        [
            GMPAccount::SEED,
            self.client_id.as_bytes(),
            &self.sender_hash,
            &*self.salt,
            std::slice::from_ref(&self.bump),
        ]
    }
}

/// Main GMP application state - matches the on-chain account structure in ics27-gmp
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, InitSpace)]
pub struct GMPAppState {
    /// Schema version for upgrades
    pub version: crate::AccountVersion,
    /// Emergency pause flag
    pub paused: bool,
    /// PDA bump seed
    pub bump: u8,
    /// Access manager program ID for role-based access control
    pub access_manager: Pubkey,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

impl GMPAppState {
    /// Seed for the main GMP application state PDA
    /// Follows the standard IBC app pattern: [`APP_STATE_SEED`, `port_id`]
    pub const SEED: &'static [u8] = b"app_state";
}
