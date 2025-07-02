//! The crate that contains the types and utilities for `tendermint-light-client-membership` program.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

use ibc_core_commitment_types::{
    commitment::CommitmentRoot,
    merkle::{MerklePath, MerkleProof},
    proto::ics23::HostFunctionsManager,
    specs::ProofSpecs,
};

#[cfg(feature = "ethereum")]
mod ethereum;

#[cfg(feature = "ethereum")]
pub use ethereum::*;

#[cfg(feature = "solana")]
mod solana;

#[cfg(feature = "solana")]
pub use solana::*;

/// Trait for abstracting key-value pair information across different platforms
pub trait KVPairInfo: Clone {
    /// Convert to merkle path and value
    fn into_merkle_path_and_value(self) -> (MerklePath, Vec<u8>);
    /// Check if this is a non-membership proof (empty value)
    fn is_non_membership(&self) -> bool;
}

/// Trait for constructing platform-specific membership outputs
pub trait MembershipOutputInfo<K> {
    /// Create output from verified key-value pairs
    fn from_verified_kvpairs(app_hash: [u8; 32], kvpairs: Vec<K>) -> Self;
}

/// Core membership verification logic
#[allow(clippy::missing_panics_doc, dead_code)]
fn membership_core<K, O>(
    app_hash: [u8; 32],
    request_iter: impl Iterator<Item = (K, MerkleProof)>,
) -> O
where
    K: KVPairInfo,
    O: MembershipOutputInfo<K>,
{
    let commitment_root = CommitmentRoot::from_bytes(&app_hash);

    let kv_pairs = request_iter
        .map(|(kv_pair, merkle_proof)| {
            let (merkle_path, value) = kv_pair.clone().into_merkle_path_and_value();

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
                        value,
                        0,
                    )
                    .unwrap();
            }

            kv_pair
        })
        .collect();

    O::from_verified_kvpairs(app_hash, kv_pairs)
}
