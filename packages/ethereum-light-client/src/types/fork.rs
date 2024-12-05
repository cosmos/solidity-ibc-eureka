use serde::{Deserialize, Serialize};

use super::wrappers::WrappedVersion;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct Fork {
    pub version: WrappedVersion,
    #[serde(default)] // TODO: REMOVE AND FIX IN E2E
    pub epoch: u64,
}
