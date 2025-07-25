use thiserror::Error;
use tonic::Status;

/// Our application-level error type.
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

impl From<AggregatorError> for Status {
    fn from(err: AggregatorError) -> Status {
        match err {
            AggregatorError::Config(msg) =>
                Status::invalid_argument(msg),

            AggregatorError::Transport(e) =>
                Status::unavailable(e.to_string()),

            AggregatorError::AttestorConnection { endpoint, source, .. } =>
                Status::unavailable(
                    format!("Failed to connect to {endpoint}: {source}")
                ),

            AggregatorError::GrpcStatus(status) =>
                status,

            AggregatorError::Timeout(ms) =>
                Status::deadline_exceeded(
                    format!("Request timed out after {ms}ms")
                ),

            AggregatorError::QuorumNotMet(req) =>
                Status::failed_precondition(
                    format!("Quorum not met: required {req}")
                ),

            AggregatorError::NoAttestationsFound(h) =>
                Status::not_found(
                    format!("No valid attestations found for height >= {h}")
                ),

            AggregatorError::Internal(msg) =>
                Status::internal(msg),

            AggregatorError::Reflection(e) =>
                Status::internal(e.to_string()),
        }
    }
}

/// Result type alias for aggregator operations
pub type Result<T> = std::result::Result<T, AggregatorError>;

/*
Error good practices:

Rust for Rustasian P:58-59
First, your error type should implement the std::error::Error trait

The main method of interest is Error::source, which provides a mechanism to find the underlying
cause of an error.


Second, your type should implement both Display and Debug 
Display should give a one-line description of what went wrong that can easily be folded
The display format should be lowercase and without trailing punctua-tion 

which #[derive(Debug)] is usually sufficient for.


Third, your type should, if possible, implement both Send and Sync so
that users are able to share the error across thread boundaries. 


Finally, where possible, your error type should be 'static. It's important that this allowes user to downcast the error to a more specific type.


In general, the community consensus is that errors should be rare and therefore should
not add much cost to the "happy path.” For that reason, errors are often placed behind
a pointer type, such as a Box or Arc. This way, they’re unlikely to add much to the size
of the overall Result type they’re contained within.


thiserror -> Libraries
anyhow -> Binaries
*/