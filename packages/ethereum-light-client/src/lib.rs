#![doc = include_str!("../README.md")]
#![deny(clippy::nursery, clippy::pedantic, warnings, missing_docs)]

pub mod client_state;
pub mod config;
pub mod consensus_state;
pub mod error;
pub mod membership;
pub mod trie;
pub mod verify;

pub mod types;

pub use typenum; // re-export (for some weird macro stuff in config.rs)

#[cfg(test)]
mod test;
