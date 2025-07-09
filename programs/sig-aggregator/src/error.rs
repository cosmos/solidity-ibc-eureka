use thiserror::Error;

#[derive(Error, Debug)]
pub enum AggregatorError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
}
