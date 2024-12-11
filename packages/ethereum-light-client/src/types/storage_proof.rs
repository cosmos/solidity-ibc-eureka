//! This module defines [`StorageProof`].

use alloy_primitives::{B256, U256};
use serde::{Deserialize, Serialize};

use super::wrappers::WrappedBytes;

/// The key-value storage proof for a smart contract account
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
pub struct StorageProof {
    /// The key of the storage
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub key: B256,
    /// The value of the storage
    #[serde(with = "ethereum_utils::base64::uint256")]
    pub value: U256,
    /// The proof of the storage
    pub proof: Vec<WrappedBytes>,
}
