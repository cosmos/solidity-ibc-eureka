//! This module defines errors for `EthApiClient`.

use alloy::transports::TransportError;

/// Error types for Ethereum API client operations
#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum EthClientError {
    /// Provider error
    #[error("provider error: {0}")]
    ProviderError(#[from] TransportError),

    /// Parse error
    #[error("parse error trying to parse {0}, {1}")]
    ParseError(String, String),

    /// Block not found error
    #[error("block not found for block number {0}")]
    BlockNotFound(u64),
}
