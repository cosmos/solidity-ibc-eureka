//! This module defines [`Height`].

use serde::{Deserialize, Serialize};

/// Height
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
pub struct Height {
    /// The revision number
    /// This is always 0 in the current implementation
    #[serde(default)]
    pub revision_number: u64,
    /// The block height
    pub revision_height: u64,
}
