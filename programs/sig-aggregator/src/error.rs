use thiserror::Error;

#[derive(Error, Debug)]
pub enum AggregatorError {
    /// Configuration-related errors
    #[error("Configuration error: {message}")]
    Config {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// gRPC transport errors (connection, network issues)
    #[error("gRPC transport error")]
    Transport {
        #[from]
        source: tonic::transport::Error,
    },

    /// gRPC status errors (server-side errors)
    #[error("gRPC status error: {code} - {message}")]
    GrpcStatus { code: tonic::Code, message: String },

    /// Errors when connecting to specific attestors
    #[error("Failed to connect to attestor '{endpoint}': {reason}")]
    AttestorConnection {
        endpoint: String,
        reason: String,
        #[source]
        source: Option<tonic::transport::Error>,
    },

    /// Quorum not met errors
    #[error("Quorum not met: required {required}")]
    QuorumNotMet { required: usize },

    /// No attestations found for requested height
    #[error("No valid attestations found for height >= {min_height}")]
    NoAttestationsFound { min_height: u64 },

    /// Request timeout errors
    #[error("Request timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Invalid attestation data
    #[error("Invalid attestation data: {reason}")]
    InvalidData { reason: String },

    /// Internal service errors
    #[error("Internal error: {message}")]
    Internal {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl AggregatorError {
    /// Create a configuration error with a simple message
    pub fn config<M: Into<String>>(message: M) -> Self {
        Self::Config {
            message: message.into(),
            source: None,
        }
    }

    /// Create a configuration error with an underlying source error
    pub fn config_with_source<M, E>(message: M, source: E) -> Self
    where
        M: Into<String>,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Config {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create an attestor connection error
    pub fn attestor_connection<E, R>(
        endpoint: E,
        reason: R,
        source: Option<tonic::transport::Error>,
    ) -> Self
    where
        E: Into<String>,
        R: Into<String>,
    {
        Self::AttestorConnection {
            endpoint: endpoint.into(),
            reason: reason.into(),
            source,
        }
    }

    /// Create a quorum not met error
    pub fn quorum_not_met(required: usize) -> Self {
        Self::QuorumNotMet { required }
    }

    /// Create a no attestations found error
    pub fn no_attestations_found(min_height: u64) -> Self {
        Self::NoAttestationsFound { min_height }
    }

    /// Create a timeout error
    pub fn timeout(timeout_ms: u64) -> Self {
        Self::Timeout { timeout_ms }
    }

    /// Create an invalid data error
    pub fn invalid_data<R: Into<String>>(reason: R) -> Self {
        Self::InvalidData {
            reason: reason.into(),
        }
    }

    /// Create an internal error with a simple message
    pub fn internal<M: Into<String>>(message: M) -> Self {
        Self::Internal {
            message: message.into(),
            source: None,
        }
    }

    /// Create an internal error with an underlying source error
    pub fn internal_with_source<M, E>(message: M, source: E) -> Self
    where
        M: Into<String>,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Internal {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Convert this error to a gRPC Status for use in tonic responses
    pub fn to_grpc_status(&self) -> tonic::Status {
        match self {
            Self::Config { message, .. } => {
                tonic::Status::invalid_argument(format!("Configuration error: {message}"))
            }
            Self::Transport { source } => {
                tonic::Status::unavailable(format!("Transport error: {source}"))
            }
            Self::GrpcStatus { code, message } => tonic::Status::new(*code, message.clone()),
            Self::AttestorConnection {
                endpoint, reason, ..
            } => tonic::Status::unavailable(format!("Attestor '{endpoint}' unavailable: {reason}")),
            Self::QuorumNotMet { required } => {
                tonic::Status::failed_precondition(format!("Quorum not met: required {required}"))
            }
            Self::NoAttestationsFound { min_height } => tonic::Status::not_found(format!(
                "No valid attestation found for height >= {min_height}"
            )),
            Self::Timeout { timeout_ms } => {
                tonic::Status::deadline_exceeded(format!("Request timeout after {timeout_ms}ms"))
            }
            Self::InvalidData { reason } => {
                tonic::Status::invalid_argument(format!("Invalid attestation data: {reason}"))
            }
            Self::Internal { message, .. } => {
                tonic::Status::internal(format!("Internal error: {message}"))
            }
        }
    }

    /// Check if this error indicates a retryable condition
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport { .. }
                | Self::AttestorConnection { .. }
                | Self::Timeout { .. }
                | Self::GrpcStatus {
                    code: tonic::Code::Unavailable | tonic::Code::DeadlineExceeded,
                    ..
                }
        )
    }

    /// Get a category string for this error
    pub fn category(&self) -> &'static str {
        match self {
            Self::Config { .. } => "configuration",
            Self::Transport { .. } | Self::GrpcStatus { .. } => "transport",
            Self::AttestorConnection { .. } => "attestor_connection",
            Self::QuorumNotMet { .. } => "quorum",
            Self::NoAttestationsFound { .. } => "no_attestations",
            Self::Timeout { .. } => "timeout",
            Self::InvalidData { .. } => "invalid_data",
            Self::Internal { .. } => "internal",
        }
    }
}

/// Result type alias for aggregator operations
pub type Result<T> = std::result::Result<T, AggregatorError>;

/// Extension trait for converting common errors to AggregatorError
pub trait IntoAggregatorError {
    fn into_aggregator_error(self) -> AggregatorError;
}

impl IntoAggregatorError for tonic::Status {
    fn into_aggregator_error(self) -> AggregatorError {
        AggregatorError::GrpcStatus {
            code: self.code(),
            message: self.message().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_error_creation_and_display() {
        let err = AggregatorError::config("invalid endpoint");
        assert!(err.to_string().contains("Configuration error"));
        assert_eq!(err.category(), "configuration");
    }

    #[test]
    fn test_error_chaining() {
        let source_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = AggregatorError::config_with_source("Failed to read config", source_err);

        assert!(err.source().is_some());
        assert_eq!(err.category(), "configuration");
    }

    #[test]
    fn test_grpc_status_conversion() {
        let err = AggregatorError::quorum_not_met(3);
        let status = err.to_grpc_status();

        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(status.message().contains("Quorum not met"));
    }

    #[test]
    fn test_retryable_classification() {
        assert!(AggregatorError::timeout(5000).is_retryable());
        assert!(
            AggregatorError::attestor_connection("http://test", "connection failed", None)
                .is_retryable()
        );
        assert!(!AggregatorError::config("bad config").is_retryable());
        assert!(!AggregatorError::invalid_data("malformed data").is_retryable());
    }

    #[test]
    fn test_error_categories() {
        assert_eq!(AggregatorError::config("test").category(), "configuration");
        assert_eq!(AggregatorError::timeout(100).category(), "timeout");
        assert_eq!(AggregatorError::quorum_not_met(3).category(), "quorum");
        assert_eq!(
            AggregatorError::no_attestations_found(100).category(),
            "no_attestations"
        );
    }

    #[test]
    fn test_convenience_constructors() {
        let err1 = AggregatorError::internal("something went wrong");
        let err2 = AggregatorError::timeout(5000);
        let err3 = AggregatorError::attestor_connection("http://test", "failed", None);

        assert!(matches!(err1, AggregatorError::Internal { .. }));
        assert!(matches!(err2, AggregatorError::Timeout { .. }));
        assert!(matches!(err3, AggregatorError::AttestorConnection { .. }));
    }
}

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