//! The crate that contains the types and utilities for `tendermint-light-client-membership` program.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

use ibc_core_commitment_types::{
    commitment::CommitmentRoot,
    merkle::{MerklePath, MerkleProof},
    proto::ics23::HostFunctionsManager,
    specs::ProofSpecs,
};
use ibc_core_host_types::path::PathBytes;

/// Platform-agnostic key-value pair for membership/non-membership proofs
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

    /// Create a non-membership proof request
    #[must_use]
    pub const fn non_membership(path: Vec<u8>) -> Self {
        Self { path, value: vec![] }
    }

    /// Check if this is a non-membership proof (empty value)
    #[must_use]
    pub fn is_non_membership(&self) -> bool {
        self.value.is_empty()
    }
}

/// Platform-agnostic output for membership verification
#[derive(Clone, Debug)]
pub struct MembershipOutput {
    /// The commitment root (app hash) that was verified
    pub commitment_root: [u8; 32],
    /// The verified key-value pairs
    pub verified_kv_pairs: Vec<KVPair>,
}

/// Verify membership and non-membership proofs
///
/// # Panics
/// Panics if proof verification fails
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn verify_membership(
    app_hash: [u8; 32],
    requests: Vec<(KVPair, MerkleProof)>,
) -> MembershipOutput {
    let commitment_root = CommitmentRoot::from_bytes(&app_hash);

    let verified_kv_pairs = requests
        .into_iter()
        .map(|(kv_pair, merkle_proof)| {
            // Convert path bytes to MerklePath
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
        verified_kv_pairs,
    }
}