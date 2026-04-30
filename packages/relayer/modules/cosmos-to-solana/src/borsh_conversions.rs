//! Conversion functions from ibc-rs types to `BorshHeader` types
//!
//! These conversions are used by the relayer to convert `Header` to `BorshHeader`
//! for efficient serialization before uploading to Solana.
//!
//! The actual implementation is in `solana-ibc-borsh-header`
//! to avoid code duplication and ensure consistency across the codebase.

pub use solana_ibc_borsh_header::conversions::*;
