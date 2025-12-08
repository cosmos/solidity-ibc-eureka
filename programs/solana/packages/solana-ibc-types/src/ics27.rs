//! ICS27 GMP (General Message Passing) types for PDA derivation
//!
//! These types are shared between the ICS27 GMP program and relayer
//! to ensure consistent PDA derivation across the system.

use anchor_lang::prelude::*;

// Re-export from solana-ibc-proto
pub use solana_ibc_proto::{
    ClientId, ConstrainedBytes, ConstrainedError, ConstrainedString, ConstrainedVec,
    GMPPacketError, GmpPacketData, Memo, Receiver, Salt, Sender, MAX_MEMO_LENGTH,
    MAX_RECEIVER_LENGTH, MAX_SALT_LENGTH, MAX_SENDER_LENGTH,
};

/// Account identifier for GMP accounts
/// The sha256 hash of this identifier is used for PDA derivation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountIdentifier {
    pub client_id: ClientId,
    pub sender: Sender,
    pub salt: Salt,
}

impl AccountIdentifier {
    /// Create a new account identifier
    pub fn new(client_id: ClientId, sender: Sender, salt: Salt) -> Self {
        Self {
            client_id,
            sender,
            salt,
        }
    }

    /// Compute sha256 hash of this identifier
    pub fn hash(&self) -> [u8; 32] {
        let mut data = Vec::new();
        data.extend_from_slice(self.client_id.as_bytes());
        data.extend_from_slice(self.sender.as_bytes());
        data.extend_from_slice(&self.salt);
        solana_sha256_hasher::hash(&data).to_bytes()
    }
}

/// GMP account for PDA derivation and signing
///
/// This type provides stateless PDA derivation for cross-chain account abstraction.
/// Each unique `AccountIdentifier` (client_id, sender, salt) derives a unique GMP account PDA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GMPAccount {
    pub account_id: AccountIdentifier,
    pub pda: Pubkey,
    pub account_bump: u8,
}

impl GMPAccount {
    /// Seed for individual account PDAs in the GMP program
    pub const SEED: &'static [u8] = b"gmp_account";

    /// Create a new GMPAccount with PDA derivation
    ///
    /// Accepts validated types, so no validation needed - construction cannot fail.
    /// The PDA is derived using the sha256 hash of the AccountIdentifier.
    pub fn new(client_id: ClientId, sender: Sender, salt: Salt, program_id: &Pubkey) -> Self {
        let account_id = AccountIdentifier::new(client_id, sender, salt);
        let (pda, account_bump) =
            Pubkey::find_program_address(&[Self::SEED, &account_id.hash()], program_id);

        Self {
            account_id,
            pda,
            account_bump,
        }
    }

    /// Get the derived PDA and bump
    pub fn pda(&self) -> (Pubkey, u8) {
        (self.pda, self.account_bump)
    }

    /// Create signer seeds for use with invoke_signed
    pub fn to_signer_seeds(&self) -> SignerSeeds {
        SignerSeeds {
            account_id_hash: self.account_id.hash(),
            bump: self.account_bump,
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
    account_id_hash: [u8; 32],
    bump: u8,
}

impl SignerSeeds {
    /// Get seeds as slices for invoke_signed
    pub fn as_slices(&self) -> [&[u8]; 3] {
        [
            GMPAccount::SEED,
            &self.account_id_hash,
            std::slice::from_ref(&self.bump),
        ]
    }
}

/// Marker type for GMP application state PDA
pub struct GMPAppState;

impl GMPAppState {
    /// Seed for the main GMP application state PDA
    /// Follows the standard IBC app pattern: [`APP_STATE_SEED`, `port_id`]
    pub const SEED: &'static [u8] = b"app_state";
}
