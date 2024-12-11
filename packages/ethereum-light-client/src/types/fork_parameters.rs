//! This module defines [`ForkParameters`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{fork::Fork, wrappers::Version};

/// The fork parameters
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct ForkParameters {
    /// The genesis fork version
    #[schemars(with = "String")]
    pub genesis_fork_version: Version,
    /// The genesis slot
    pub genesis_slot: u64,
    /// The altair fork
    pub altair: Fork,
    /// The bellatrix fork
    pub bellatrix: Fork,
    /// The capella fork
    pub capella: Fork,
    /// The deneb fork
    pub deneb: Fork,
}

impl ForkParameters {
    /// Returns the fork version based on the `epoch`.
    /// NOTE: This implementation is based on capella.
    ///
    /// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/capella/fork.md#modified-compute_fork_version)
    #[must_use]
    pub const fn compute_fork_version(&self, epoch: u64) -> Version {
        match epoch {
            _ if epoch >= self.deneb.epoch => self.deneb.version,
            _ if epoch >= self.capella.epoch => self.capella.version,
            _ if epoch >= self.bellatrix.epoch => self.bellatrix.version,
            _ if epoch >= self.altair.epoch => self.altair.version,
            _ => self.genesis_fork_version,
        }
    }
}
