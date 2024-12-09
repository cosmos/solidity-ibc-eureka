use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct Height {
    #[serde(default)]
    pub revision_number: u64,
    pub revision_height: u64,
}
