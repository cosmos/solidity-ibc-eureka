//! Defines the [`ContractError`] type.

use attestor_light_client::error::IbcAttestorClientError;
use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(missing_docs, clippy::module_name_repetitions)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("client state latest height and height are not equal")]
    ClientStateHeightMismatch,

    #[error("client and consensus state mismatch")]
    ClientAndConsensusStateMismatch,

    #[error("serializing client state failed: {0}")]
    SerializeClientStateFailed(#[source] serde_json::Error),

    #[error("serializing consensus state failed: {0}")]
    SerializeConsensusStateFailed(#[source] serde_json::Error),

    #[error("deserializing client state failed: {0}")]
    DeserializeClientStateFailed(#[source] serde_json::Error),

    #[error("deserializing consensus state failed: {0}")]
    DeserializeConsensusStateFailed(#[source] serde_json::Error),

    #[error("deserializing client message failed: {0}")]
    DeserializeClientMessageFailed(#[source] serde_json::Error),

    #[error("verify membership failed: {0}")]
    VerifyMembershipFailed(#[source] IbcAttestorClientError),

    #[error("verify non-membership failed: {0}")]
    VerifyNonMembershipFailed(#[source] IbcAttestorClientError),

    #[error("verify client message failed: {0}")]
    VerifyClientMessageFailed(#[source] IbcAttestorClientError),

    #[error("update client state failed: {0}")]
    UpdateClientStateFailed(#[source] IbcAttestorClientError),

    #[error("client state not found")]
    ClientStateNotFound,

    #[error("consensus state not found")]
    ConsensusStateNotFound,

    // Generic translation errors
    #[error("prost encoding error: {0}")]
    ProstEncodeError(#[from] prost::EncodeError),

    #[error("prost decoding error: {0}")]
    ProstDecodeError(#[from] prost::DecodeError),

    #[error("serde json error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),

    #[error("timestamp too large overflow")]
    TimestampOverflow,

    #[error("invalid client message")]
    InvalidClientMessage,
}
