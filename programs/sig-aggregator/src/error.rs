use thiserror::Error;
use tonic::Status;

#[derive(Debug, Error)]
pub enum AggregatorError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),

    #[error("reflection build error: {0}")]
    Reflection(#[from] tonic_reflection::server::Error),

    #[error(transparent)]
    GrpcStatus(#[from] Status),

    #[error("attestor {endpoint} connection failed: {source}")]
    AttestorConnection {
        endpoint: String,
        #[source]
        source: tonic::transport::Error,
    },

    #[error("quorum not met: required {0}")]
    QuorumNotMet(usize),

    #[error("no valid attestations found for height >= {0}")]
    NoAttestationsFound(u64),

    #[error("request timed out after {0}ms")]
    Timeout(u64),

    #[error("internal error: {0}")]
    Internal(String),
}
