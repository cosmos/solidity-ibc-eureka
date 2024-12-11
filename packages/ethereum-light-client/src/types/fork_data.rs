//! This module defines [`compute_fork_data_root`].

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::types::wrappers::Version;

/// The fork data
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default, TreeHash)]
struct ForkData {
    /// The current version
    pub current_version: Version,
    /// The genesis validators root
    pub genesis_validators_root: B256,
}

/// Return the 32-byte fork data root for the `current_version` and `genesis_validators_root`.
/// This is used primarily in signature domains to avoid collisions across forks/chains.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#compute_fork_data_root)
#[must_use]
pub fn compute_fork_data_root(current_version: Version, genesis_validators_root: B256) -> B256 {
    let fork_data = ForkData {
        current_version,
        genesis_validators_root,
    };

    fork_data.tree_hash_root()
}
