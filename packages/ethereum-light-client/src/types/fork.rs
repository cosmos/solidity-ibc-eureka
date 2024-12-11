//! This module defines [`Fork`].

use serde::{Deserialize, Serialize};

use super::wrappers::WrappedVersion;

/// The fork data
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
pub struct Fork {
    /// The version of the fork
    pub version: WrappedVersion,
    /// The epoch at which this fork is activated
    #[serde(default)] // TODO: Remove this when doing e2e integration #143
    pub epoch: u64,
}
