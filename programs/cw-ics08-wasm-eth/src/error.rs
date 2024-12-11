//! Defines the [`ContractError`] type.

use cosmwasm_std::StdError;
use ethereum_light_client::error::EthereumIBCError;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(missing_docs, clippy::module_name_repetitions)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("deserializing client state failed: {0}")]
    DeserializeClientStateFailed(#[source] serde_json::Error),

    #[error("deserializing consensus state failed: {0}")]
    DeserializeConsensusStateFailed(#[source] serde_json::Error),

    #[error("Verify membership failed: {0}")]
    VerifyMembershipFailed(#[source] EthereumIBCError),

    #[error("Verify non-membership failed: {0}")]
    VerifyNonMembershipFailed(#[source] EthereumIBCError),

    #[error("Verify client message failed: {0}")]
    VerifyClientMessageFailed(#[source] EthereumIBCError),

    #[error("prost encoding error: {0}")]
    ProstEncodeError(#[from] prost::EncodeError),

    #[error("prost decoding error: {0}")]
    ProstDecodeError(#[from] prost::DecodeError),
}
