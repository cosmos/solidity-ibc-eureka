//! This module defines [`Fork`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::wrappers::Version;

/// The fork data
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct Fork {
    /// The version of the fork
    #[schemars(with = "String")]
    pub version: Version,
    /// The epoch at which this fork is activated
    pub epoch: u64,
}
