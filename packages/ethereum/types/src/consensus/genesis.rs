//! This module defines types related to the genesis endpoint of the Beacon API.

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use super::fork::Version;

/// Genesis provides information about the genesis of a chain.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
pub struct Genesis {
    /// The genesis time (in unix seconds)
    pub genesis_time: u64,
    /// The genesis validators root
    pub genesis_validators_root: B256,
    /// The genesis fork version
    pub genesis_fork_version: Version,
}
