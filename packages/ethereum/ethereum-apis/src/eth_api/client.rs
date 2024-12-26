use std::str::FromStr;

use alloy_primitives::{Address, StorageKey};
use alloy_provider::Provider;
use alloy_rpc_types_eth::EIP1186AccountProofResponse;
use alloy_transport::{Transport, TransportError};

pub struct EthApiClient<T: Transport + Clone, P: Provider<T> + Clone> {
    provider: P,
    _transport: std::marker::PhantomData<T>,
}

#[derive(Debug, thiserror::Error)]
pub enum EthGetProofError {
    #[error("provider error: {0}")]
    ProviderError(#[from] TransportError),

    #[error("parse error: {0}")]
    ParseError(String),
}

impl<T: Transport + Clone, P: Provider<T> + Clone> EthApiClient<T, P> {
    pub const fn new(provider: P) -> Self {
        Self {
            provider,
            _transport: std::marker::PhantomData,
        }
    }

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
            .request("get_proof", (address, storage_keys, block_hex))
            .await?)
    }

    pub async fn get_block_number(&self) -> Result<u64, EthGetProofError> {
        Ok(self.provider.get_block_number().await?)
    }
}
