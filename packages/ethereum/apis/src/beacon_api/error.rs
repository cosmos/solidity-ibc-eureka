//! This module defines errors for `BeaconApiClient`.

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

/// The error type for the beacon api client.
#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum BeaconApiClientError {
    /// HTTP request error
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON deserialization error
    #[error("json deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Resource not found error
    #[error("not found: {0}")]
    NotFound(#[from] NotFoundError),

    /// Internal server error
    #[error("internal error: {0}")]
    Internal(#[from] InternalServerError),

    /// Other unknown error
    #[error("unknown error ({code}): {text}")]
    Other {
        /// HTTP status code
        code: StatusCode,
        /// Error text
        text: String,
    },
}

/// The not found error structure returned by the Beacon API.
#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
#[error("{status_code} {error}: {message}")]
#[allow(clippy::module_name_repetitions)]
pub struct NotFoundError {
    /// HTTP status code
    #[serde(rename = "statusCode")]
    pub status_code: u64,
    /// Error type
    pub error: String,
    /// Error message
    pub message: String,
}

/// The internal server error returned by the Beacon API.
#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
#[error("{status_code} {error}: {message}")]
#[allow(clippy::module_name_repetitions)]
pub struct InternalServerError {
    /// HTTP status code
    #[serde(rename = "statusCode")]
    pub status_code: u64,
    /// Error type
    pub error: String,
    /// Error message
    pub message: String,
}
