//! Protobuf types for GMP relayer
//!
//! This module re-exports types from the shared solana-ibc-proto crate.
//! Proto generation is centralized to ensure type consistency across programs and relayer.

// Re-export from shared proto crate
pub use solana_ibc_proto::{
    GmpAcknowledgement as GmpPacketDataAcknowledgement, GmpPacketData, GmpSolanaPayload,
    SolanaAccountMeta, ValidatedGMPSolanaPayload,
};
