//! This module defines [`ForkParameters`].

use serde::{Deserialize, Serialize};

use super::{fork::Fork, wrappers::WrappedVersion};

/// The fork parameters
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
pub struct ForkParameters {
    /// The genesis fork version
    pub genesis_fork_version: WrappedVersion,
    /// The genesis slot
    #[serde(default)] // TODO: REMOVE AND FIX IN E2E
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
    pub fn compute_fork_version(&self, epoch: u64) -> WrappedVersion {
        match epoch {
            _ if epoch >= self.deneb.epoch => self.deneb.version.clone(),
            _ if epoch >= self.capella.epoch => self.capella.version.clone(),
            _ if epoch >= self.bellatrix.epoch => self.bellatrix.version.clone(),
            _ if epoch >= self.altair.epoch => self.altair.version.clone(),
            _ => self.genesis_fork_version.clone(),
        }
    }
}
