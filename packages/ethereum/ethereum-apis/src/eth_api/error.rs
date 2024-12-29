//! This module defines errors for `EthApiClient`.

use alloy_transport::TransportError;

#[derive(Debug, thiserror::Error)]
#[allow(missing_docs, clippy::module_name_repetitions)]
pub enum EthGetProofError {
    #[error("provider error: {0}")]
    ProviderError(#[from] TransportError),

    #[error("parse error: {0}")]
    ParseError(String),
}
