use serde::{Deserialize, Serialize};

use super::wrappers::WrappedVersion;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct Fork {
    pub version: WrappedVersion,
    #[serde(default)] // TODO: Remove this when doing e2e integration #143
    pub epoch: u64,
}