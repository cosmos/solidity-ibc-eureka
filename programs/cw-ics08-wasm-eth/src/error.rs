//! Defines the [`ContractError`] type.

use cosmwasm_std::StdError;
use ethereum_light_client::error::EthereumIBCError;
use thiserror::Error;

/// Error types that can be returned by contract operations
#[derive(Error, Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum ContractError {
    /// Standard `CosmWasm` error
    #[error("{0}")]
    Std(#[from] StdError),

    /// Client state latest height and slot are not equal
    #[error("client state latest height and slot are not equal")]
    ClientStateSlotMismatch,

    /// Client and consensus state mismatch
    #[error("client and consensus state mismatch")]
    ClientAndConsensusStateMismatch,

    /// Serializing client state failed
    #[error("serializing client state failed: {0}")]
    SerializeClientStateFailed(#[source] serde_json::Error),

    /// Serializing consensus state failed
    #[error("serializing consensus state failed: {0}")]
    SerializeConsensusStateFailed(#[source] serde_json::Error),

    /// Deserializing client state failed
    #[error("deserializing client state failed: {0}")]
    DeserializeClientStateFailed(#[source] serde_json::Error),

    /// Deserializing consensus state failed
    #[error("deserializing consensus state failed: {0}")]
    DeserializeConsensusStateFailed(#[source] serde_json::Error),

    /// Deserializing client message failed
    #[error("deserializing client message failed: {0}")]
    DeserializeClientMessageFailed(#[source] serde_json::Error),

    /// Deserializing ethereum misbehaviour message failed
    #[error("deserializing ethereum misbehaviour message failed: {0}")]
    DeserializeEthMisbehaviourFailed(#[source] serde_json::Error),

    /// Verify membership failed
    #[error("verify membership failed: {0}")]
    VerifyMembershipFailed(#[source] EthereumIBCError),

    /// Verify non-membership failed
    #[error("verify non-membership failed: {0}")]
    VerifyNonMembershipFailed(#[source] EthereumIBCError),

    /// Verify client message failed
    #[error("verify client message failed: {0}")]
    VerifyClientMessageFailed(#[source] EthereumIBCError),

    /// Update client state failed
    #[error("update client state failed: {0}")]
    UpdateClientStateFailed(#[source] EthereumIBCError),

    /// Unsupported fork version
    #[error("unsupported fork version")]
    UnsupportedForkVersion(#[source] EthereumIBCError),

    /// Client state not found
    #[error("client state not found")]
    ClientStateNotFound,

    /// Consensus state not found
    #[error("consensus state not found")]
    ConsensusStateNotFound,

    // Generic translation errors
    /// Prost encoding error
    #[error("prost encoding error: {0}")]
    ProstEncodeError(#[from] prost::EncodeError),

    /// Prost decoding error
    #[error("prost decoding error: {0}")]
    ProstDecodeError(#[from] prost::DecodeError),

    /// Serde JSON error
    #[error("serde json error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),

    /// Invalid client message
    #[error("invalid client message")]
    InvalidClientMessage,
}
