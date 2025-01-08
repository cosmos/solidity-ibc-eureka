//! This module implements the `EthApiClient` to interact with the Ethereum RPC API.

use std::str::FromStr;

use alloy::{
    primitives::{Address, StorageKey},
    providers::Provider,
    rpc::types::EIP1186AccountProofResponse,
    transports::Transport,
};

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
    #[tracing::instrument(skip_all)]
    pub async fn get_proof(
        &self,
        address: &str,
        storage_keys: Vec<String>,
        block_hex: String,
    ) -> Result<EIP1186AccountProofResponse, EthGetProofError> {
        tracing::debug!("get_proof {} {:?} {}", address, storage_keys, block_hex);

        let address: Address = Address::from_str(address)
            .map_err(|e| EthGetProofError::ParseError(address.to_string(), e.to_string()))?;
        let storage_keys: Vec<StorageKey> = storage_keys
            .into_iter()
            .map(|key| {
                StorageKey::from_str(&key)
                    .map_err(|e| EthGetProofError::ParseError(key, e.to_string()))
            })
            .collect::<Result<_, _>>()?;
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
