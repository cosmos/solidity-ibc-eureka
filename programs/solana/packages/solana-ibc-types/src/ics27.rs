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
    ///
    /// Uses Borsh serialization to ensure deterministic, collision-resistant encoding.
    /// Borsh automatically length-prefixes variable-length fields (strings use u32 length prefix).
    pub fn hash(&self) -> [u8; 32] {
        use borsh::BorshSerialize;

        // Wrapper struct for Borsh serialization
        // Borsh encodes String as: u32 length + bytes
        // Borsh encodes Vec<u8> as: u32 length + bytes
        #[derive(BorshSerialize)]
        struct Hashable {
            client_id: String,
            sender: String,
            salt: Vec<u8>,
        }

        let hashable = Hashable {
            client_id: self.client_id.to_string(),
            sender: self.sender.to_string(),
            salt: self.salt.to_vec(),
        };

        let data = borsh::to_vec(&hashable).expect("borsh serialization cannot fail");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that different client_id/sender boundaries produce different hashes.
    /// This verifies the length-prefix fix prevents collision attacks.
    #[test]
    fn test_no_collision_different_boundaries() {
        // Case 1: client_id="ab", sender="cdef"
        let id1 = AccountIdentifier::new(
            "ab".to_string().try_into().unwrap(),
            "cdef".to_string().try_into().unwrap(),
            vec![].try_into().unwrap(),
        );

        // Case 2: client_id="abc", sender="def" - different logical values
        let id2 = AccountIdentifier::new(
            "abc".to_string().try_into().unwrap(),
            "def".to_string().try_into().unwrap(),
            vec![].try_into().unwrap(),
        );

        // With length-prefix fix: these produce DIFFERENT hashes
        assert_ne!(
            id1.hash(),
            id2.hash(),
            "Different field boundaries must produce different hashes"
        );
    }

    /// Test that sender/salt boundary shifts produce different hashes.
    #[test]
    fn test_no_collision_sender_salt_boundary() {
        // sender="abc", salt=[0x64, 0x65, 0x66] ("def" in ASCII)
        let id1 = AccountIdentifier::new(
            "client".to_string().try_into().unwrap(),
            "abc".to_string().try_into().unwrap(),
            vec![0x64, 0x65, 0x66].try_into().unwrap(),
        );

        // sender="abcdef", salt=[]
        let id2 = AccountIdentifier::new(
            "client".to_string().try_into().unwrap(),
            "abcdef".to_string().try_into().unwrap(),
            vec![].try_into().unwrap(),
        );

        // With length-prefix fix: these produce DIFFERENT hashes
        assert_ne!(
            id1.hash(),
            id2.hash(),
            "Different sender/salt boundaries must produce different hashes"
        );
    }

    /// Test that truly different identifiers produce different hashes
    #[test]
    fn test_different_identifiers_different_hashes() {
        let id1 = AccountIdentifier::new(
            "07-tendermint-0".to_string().try_into().unwrap(),
            "cosmos1abc".to_string().try_into().unwrap(),
            vec![1, 2, 3].try_into().unwrap(),
        );

        let id2 = AccountIdentifier::new(
            "07-tendermint-1".to_string().try_into().unwrap(),
            "cosmos1abc".to_string().try_into().unwrap(),
            vec![1, 2, 3].try_into().unwrap(),
        );

        assert_ne!(
            id1.hash(),
            id2.hash(),
            "Different client_id should produce different hash"
        );
    }
}
