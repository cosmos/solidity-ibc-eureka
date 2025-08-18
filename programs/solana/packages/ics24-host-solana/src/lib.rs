//! ICS-24 Host Chain Requirements for IBC on Solana
//!
//! This crate defines the common types and messages required for implementing
//! IBC host chain functionality on Solana, as specified by ICS-24.
//!
//! It provides shared interfaces for IBC application callbacks and packet handling.

pub mod app_msgs;

// Re-export commonly used types
pub use app_msgs::{IBCAppError, OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg, Payload};

