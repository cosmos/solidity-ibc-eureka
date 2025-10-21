//! Generated Protobuf types for GMP relayer
//!
//! This module contains types generated from .proto files via prost-build.
//! The proto files are located in proto/gmp/ and proto/solana/.

// Include generated code from build.rs
pub mod gmp {
    include!(concat!(env!("OUT_DIR"), "/gmp.rs"));
}

pub mod solana {
    include!(concat!(env!("OUT_DIR"), "/solana.rs"));
}

// Re-export for convenience
pub use gmp::GmpPacketData;
pub use solana::{SolanaAccountMeta, SolanaInstruction};
