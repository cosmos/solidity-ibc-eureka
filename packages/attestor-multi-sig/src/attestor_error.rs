use thiserror::Error;

#[derive(Error, Debug)]
pub enum AttestorError {
    #[error("Failed to fetch header: {0}")]
    FetchHeader(String),
    #[error("Failed to fetch proof: {0}")]
    FetchProof(String),
    #[error("Signing error: {0}")]
    Signing(String),
}
