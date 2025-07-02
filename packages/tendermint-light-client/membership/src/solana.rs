//! Solana-specific types and implementations for membership program

use ibc_core_commitment_types::merkle::{MerklePath, MerkleProof};
use ibc_core_host_types::path::PathBytes;

use crate::{membership_core, KVPairInfo, MembershipOutputInfo};

/// Solana-specific key-value pair for membership/non-membership proofs
#[derive(Clone, Debug)]
pub struct SolanaKVPair {
    /// The storage path
    pub path: Vec<u8>,
    /// The value
    pub value: Vec<u8>,
}

impl KVPairInfo for SolanaKVPair {
    fn into_merkle_path_and_value(self) -> (MerklePath, Vec<u8>) {
        // Parse the path bytes into a MerklePath
        // The path format should follow IBC path conventions
        let path = PathBytes::from_bytes(self.path);
        let merkle_path = MerklePath::new(vec![path]);

        (merkle_path, self.value)
    }

    fn is_non_membership(&self) -> bool {
        self.value.is_empty()
    }
}

/// Output for the membership program on Solana
#[derive(Clone, Debug)]
pub struct SolanaMembershipOutput {
    /// The app hash that was verified against
    pub app_hash: [u8; 32],
    /// The verified key-value pairs
    pub verified_kv_pairs: Vec<SolanaKVPair>,
}

impl MembershipOutputInfo<SolanaKVPair> for SolanaMembershipOutput {
    fn from_verified_kvpairs(app_hash: [u8; 32], kvpairs: Vec<SolanaKVPair>) -> Self {
        Self {
            app_hash,
            verified_kv_pairs: kvpairs,
        }
    }
}

/// Helper to create a membership proof request
#[must_use]
pub const fn create_membership_request(path: Vec<u8>, value: Vec<u8>) -> SolanaKVPair {
    SolanaKVPair { path, value }
}

/// Helper to create a non-membership proof request
#[must_use]
pub const fn create_non_membership_request(path: Vec<u8>) -> SolanaKVPair {
    SolanaKVPair {
        path,
        value: vec![],
    }
}

/// The main membership verification function for Solana
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn membership(
    app_hash: [u8; 32],
    request_iter: impl Iterator<Item = (SolanaKVPair, MerkleProof)>,
) -> SolanaMembershipOutput {
    membership_core(app_hash, request_iter)
}
