//! The crate that contains the types and utilities for `tendermint-light-client-membership` program.
//!
//! This crate provides dual APIs for native and zkVM environments:
//!
//! - **Native**: Returns `Result<T, E>` for proper error handling and composability
//! - **zkVM**: Returns `T` directly, panics on error to avoid Result overhead (~50-100 cycles)
//!
//! In zkVM, Result types generate unnecessary constraints even when immediately unwrapped.
//! Since proofs either succeed or fail entirely, error recovery is meaningless and wasteful.
//!
//! Use `--features panic` for zkVM builds to get optimal performance.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

use ibc_core_commitment_types::{
    commitment::CommitmentRoot,
    merkle::{MerklePath, MerkleProof},
    proto::ics23::HostFunctionsManager,
    specs::ProofSpecs,
};
use ibc_core_host_types::path::PathBytes;

/// Key-value pair for membership/non-membership proofs
#[derive(Clone, Debug)]
pub struct KVPair {
    /// Storage path as raw bytes
    pub path: Vec<u8>,
    /// Value (empty for non-membership proofs)
    pub value: Vec<u8>,
}

impl KVPair {
    /// Create a new key-value pair
    #[must_use]
    pub const fn new(path: Vec<u8>, value: Vec<u8>) -> Self {
        Self { path, value }
    }

    /// Check if this is a non-membership proof (empty value)
    #[must_use]
    pub fn is_non_membership(&self) -> bool {
        self.value.is_empty()
    }
}

/// Output for membership verification
#[derive(Clone, Debug)]
pub struct MembershipOutput {
    /// The commitment root (app hash) that was verified
    pub commitment_root: [u8; 32],
    /// The verified key-value pairs
    pub kv_pairs: Vec<KVPair>,
}

/// Error type for membership verification (only used when panic feature is not enabled)
#[cfg(not(feature = "panic"))]
#[derive(Debug, thiserror::Error)]
pub enum MembershipError {
    /// Non-membership verification failed
    #[error("non-membership verification failed")]
    NonMembershipVerificationFailed,
    /// Membership verification failed
    #[error("membership verification failed")]
    MembershipVerificationFailed,
}

/// IBC membership verification - panic version
#[cfg(feature = "panic")]
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn membership(
    app_hash: [u8; 32],
    request_iter: impl Iterator<Item = (KVPair, MerkleProof)>,
) -> MembershipOutput {
    let commitment_root = CommitmentRoot::from_bytes(&app_hash);

    let kv_pairs = request_iter
        .map(|(kv_pair, merkle_proof)| {
            let path = PathBytes::from_bytes(kv_pair.path.clone());
            let merkle_path = MerklePath::new(vec![path]);

            if kv_pair.is_non_membership() {
                merkle_proof
                    .verify_non_membership::<HostFunctionsManager>(
                        &ProofSpecs::cosmos(),
                        commitment_root.clone().into(),
                        merkle_path,
                    )
                    .unwrap();
            } else {
                merkle_proof
                    .verify_membership::<HostFunctionsManager>(
                        &ProofSpecs::cosmos(),
                        commitment_root.clone().into(),
                        merkle_path,
                        kv_pair.value.clone(),
                        0,
                    )
                    .unwrap();
            }

            kv_pair
        })
        .collect();

    MembershipOutput {
        commitment_root: app_hash,
        kv_pairs,
    }
}

/// IBC membership verification - non-panic version
#[cfg(not(feature = "panic"))]
#[must_use]
pub fn membership(
    app_hash: [u8; 32],
    request_iter: impl Iterator<Item = (KVPair, MerkleProof)>,
) -> Result<MembershipOutput, MembershipError> {
    let commitment_root = CommitmentRoot::from_bytes(&app_hash);

    let kv_pairs = request_iter
        .map(|(kv_pair, merkle_proof)| {
            let path = PathBytes::from_bytes(kv_pair.path.clone());
            let merkle_path = MerklePath::new(vec![path]);

            if kv_pair.is_non_membership() {
                merkle_proof
                    .verify_non_membership::<HostFunctionsManager>(
                        &ProofSpecs::cosmos(),
                        commitment_root.clone().into(),
                        merkle_path,
                    )
                    .map_err(|_| MembershipError::NonMembershipVerificationFailed)?;
            } else {
                merkle_proof
                    .verify_membership::<HostFunctionsManager>(
                        &ProofSpecs::cosmos(),
                        commitment_root.clone().into(),
                        merkle_path,
                        kv_pair.value.clone(),
                        0,
                    )
                    .map_err(|_| MembershipError::MembershipVerificationFailed)?;
            }

            Ok(kv_pair)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MembershipOutput {
        commitment_root: app_hash,
        kv_pairs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibc_core_commitment_types::merkle::MerkleProof;

    fn dummy_merkle_proof() -> MerkleProof {
        MerkleProof {
            proofs: vec![],
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_with_invalid_merkle_proof() {
        let app_hash = [1u8; 32];
        let kv_pairs = vec![
            (
                KVPair::new(b"key1".to_vec(), b"value1".to_vec()),
                dummy_merkle_proof(),
            ),
        ];

        #[cfg(feature = "panic")]
        {
            let result = std::panic::catch_unwind(|| {
                membership(app_hash, kv_pairs.into_iter())
            });
            assert!(result.is_err(), "Expected panic for invalid proof");
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = membership(app_hash, kv_pairs.into_iter());
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_non_membership_with_invalid_proof() {
        let app_hash = [2u8; 32];
        let kv_pairs = vec![
            (
                KVPair::new(b"key1".to_vec(), vec![]), // empty value = non-membership
                dummy_merkle_proof(),
            ),
        ];

        #[cfg(feature = "panic")]
        {
            // This will panic due to invalid proof
            let result = std::panic::catch_unwind(|| {
                membership(app_hash, kv_pairs.into_iter())
            });
            assert!(result.is_err(), "Expected panic for invalid proof");
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = membership(app_hash, kv_pairs.into_iter());
            assert!(result.is_err());
            match result {
                Err(MembershipError::NonMembershipVerificationFailed) => {
                    // Expected error
                }
                _ => panic!("Unexpected error type"),
            }
        }
    }

    #[test]
    fn test_kv_pair_is_non_membership() {
        let membership_kv = KVPair::new(b"key".to_vec(), b"value".to_vec());
        assert!(!membership_kv.is_non_membership());

        let non_membership_kv = KVPair::new(b"key".to_vec(), vec![]);
        assert!(non_membership_kv.is_non_membership());
    }
}
