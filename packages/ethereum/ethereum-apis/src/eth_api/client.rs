//! This module implements the `EthApiClient` to interact with the Ethereum RPC API.

use std::str::FromStr;

use alloy_primitives::{Address, StorageKey};
use alloy_provider::Provider;
use alloy_rpc_types_eth::EIP1186AccountProofResponse;
use alloy_transport::Transport;

use super::error::EthGetProofError;

const RPC_METHOD_GET_PROOF: &str = "eth_getProof";

/// The api client for interacting with the Beacon API
#[allow(clippy::module_name_repetitions)]
pub struct EthApiClient<T: Transport + Clone, P: Provider<T> + Clone> {
    provider: P,
    _transport: std::marker::PhantomData<T>,
}

impl<T: Transport + Clone, P: Provider<T> + Clone> EthApiClient<T, P> {
    /// Create new `EthApiClient`
    pub const fn new(provider: P) -> Self {
        Self {
            provider,
            _transport: std::marker::PhantomData,
        }
    }

    /// Fetches proof for an account and optionally storage keys under the given account at the block.
    /// # Errors
    /// Returns an error if the input fails to serialize, the request fails or the response is not successful deserialized
    pub async fn get_proof(
        &self,
        address: &str,
        storage_keys: Vec<String>,
        block_hex: String,
    ) -> Result<EIP1186AccountProofResponse, EthGetProofError> {
        let address: Address =
            Address::from_str(address).map_err(|e| EthGetProofError::ParseError(e.to_string()))?;
        let storage_keys: Vec<StorageKey> = storage_keys
            .into_iter()
            .map(|key| StorageKey::from_str(&key))
            .collect::<Result<_, _>>()
            .map_err(|e| EthGetProofError::ParseError(e.to_string()))?;
        Ok(self
            .provider
            .client()
            .request(RPC_METHOD_GET_PROOF, (address, storage_keys, block_hex))
            .await?)
    }

    /// Fetches the current block number.
    /// # Errors
    /// Returns an error if the request fails
    pub async fn get_block_number(&self) -> Result<u64, EthGetProofError> {
        Ok(self.provider.get_block_number().await?)
    }
}
