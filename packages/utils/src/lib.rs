#![doc = include_str!("../README.md")]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

pub mod eth;
pub mod light_block;
pub mod merkle;
pub mod rpc;
