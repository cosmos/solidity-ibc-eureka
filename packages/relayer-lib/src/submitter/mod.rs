//! This module defines the [`ChainSubmitterService`] trait and some of its implementations.
//! This interface is used to generate proofs and submit transactions to a chain.

#[cfg(feature = "sp1-toolchain")]
pub mod eth_eureka;
mod r#trait;

pub use r#trait::ChainSubmitterService;
