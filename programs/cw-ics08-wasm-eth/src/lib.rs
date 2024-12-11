#![doc = include_str!("../README.md")]
#![deny(clippy::nursery, clippy::pedantic, warnings, missing_docs)]

pub mod contract;
pub mod custom_query;
mod error;
#[allow(clippy::module_name_repetitions)]
pub mod msg;
pub mod state;

pub use error::ContractError;

#[cfg(test)]
mod test;
