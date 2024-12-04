use serde::{Deserialize, Serialize};

use super::{fork::Fork, wrappers::Version};

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct ForkParameters {
    pub genesis_fork_version: Version,
    #[serde(default)] // TODO: REMOVE AND FIX IN E2E
    pub genesis_slot: u64,
    pub altair: Fork,
    pub bellatrix: Fork,
    pub capella: Fork,
    pub deneb: Fork,
}

/// Returns the fork version based on the `epoch` and `fork_parameters`.
/// NOTE: This implementation is based on capella.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/capella/fork.md#modified-compute_fork_version)
pub fn compute_fork_version(fork_parameters: &ForkParameters, epoch: u64) -> Version {
    if epoch >= fork_parameters.deneb.epoch {
        fork_parameters.deneb.version.clone()
    } else if epoch >= fork_parameters.capella.epoch {
        fork_parameters.capella.version.clone()
    } else if epoch >= fork_parameters.bellatrix.epoch {
        fork_parameters.bellatrix.version.clone()
    } else if epoch >= fork_parameters.altair.epoch {
        fork_parameters.altair.version.clone()
    } else {
        fork_parameters.genesis_fork_version.clone()
    }
}
