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
        let sender_hash = solana_sha256_hasher::hash(sender.as_bytes()).to_bytes();
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

/// Marker type for GMP application state PDA
pub struct GMPAppState;

impl GMPAppState {
    /// Seed for the main GMP application state PDA
    /// Follows the standard IBC app pattern: [`APP_STATE_SEED`, `port_id`]
    pub const SEED: &'static [u8] = b"app_state";
}

/// Status of a GMP call result.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum CallResultStatus {
    /// The call received an acknowledgement from the destination chain.
    Acknowledgement,
    /// The call timed out before being processed.
    Timeout,
}

/// GMP call result PDA derivation helper.
///
/// This type provides stateless PDA derivation for GMP call results.
/// The PDA stores the acknowledgement or timeout result of a GMP call.
///
/// # PDA Seeds
/// `["gmp_result", source_client, sequence (little-endian u64)]`
pub struct GMPCallResult;

impl GMPCallResult {
    /// Seed prefix for GMP call result PDAs.
    pub const SEED: &'static [u8] = b"gmp_result";

    /// Derive the PDA for a GMP call result.
    ///
    /// # Arguments
    /// * `source_client` - The source client ID (light client on Solana tracking the source chain)
    /// * `sequence` - The IBC packet sequence number
    /// * `program_id` - The GMP program ID
    ///
    /// # Returns
    /// A tuple of (PDA pubkey, bump seed)
    pub fn pda(source_client: &str, sequence: u64, program_id: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::SEED,
                source_client.as_bytes(),
                &sequence.to_le_bytes(),
            ],
            program_id,
        )
    }
}
