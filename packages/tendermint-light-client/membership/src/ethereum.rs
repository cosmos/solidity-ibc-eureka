//! Ethereum-specific implementations for membership verification

use crate::{membership_core, KVPairInfo, MembershipOutputInfo};
use ibc_core_commitment_types::merkle::{MerklePath, MerkleProof};
use ibc_eureka_solidity_types::msgs::IMembershipMsgs::{KVPair, MembershipOutput};

impl KVPairInfo for KVPair {
    fn into_merkle_path_and_value(self) -> (MerklePath, Vec<u8>) {
        self.into()
    }

    fn is_non_membership(&self) -> bool {
        self.value.is_empty()
    }
}

impl MembershipOutputInfo<KVPair> for MembershipOutput {
    fn from_verified_kvpairs(app_hash: [u8; 32], kvpairs: Vec<KVPair>) -> Self {
        MembershipOutput {
            commitmentRoot: app_hash.into(),
            kvPairs: kvpairs,
        }
    }
}

/// The main function of the program without the zkVM wrapper.
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn membership(
    app_hash: [u8; 32],
    request_iter: impl Iterator<Item = (KVPair, MerkleProof)>,
) -> MembershipOutput {
    membership_core(app_hash, request_iter)
}
