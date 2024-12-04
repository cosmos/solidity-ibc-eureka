use alloy_primitives::{B256, U256};
use serde::{Deserialize, Serialize};

use super::wrappers::MyBytes;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct StorageProof {
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub key: B256,
    #[serde(with = "ethereum_utils::base64::uint256")]
    pub value: U256,
    pub proof: Vec<MyBytes>,
}
