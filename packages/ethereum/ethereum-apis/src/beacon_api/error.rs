use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum BeaconApiClientError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("not found: {0}")]
    NotFound(#[from] NotFoundError),

    #[error("internal error: {0}")]
    Internal(#[from] InternalServerError),

    #[error("unknown error ({code}): {text}")]
    Other { code: StatusCode, text: String },
}

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
#[error("{status_code} {error}: {message}")]
pub struct NotFoundError {
    #[serde(rename = "statusCode")]
    pub status_code: u64,
    pub error: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
#[error("{status_code} {error}: {message}")]
pub struct InternalServerError {
    #[serde(rename = "statusCode")]
    pub status_code: u64,
    pub error: String,
    pub message: String,
}
