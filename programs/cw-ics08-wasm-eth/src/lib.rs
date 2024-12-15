#![doc = include_str!("../README.md")]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

pub mod contract;
pub mod custom_query;
mod error;
pub mod msg;
pub mod state;

pub use error::ContractError;
