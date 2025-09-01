#![doc = include_str!("../README.md")]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

pub mod aggregator;
pub mod chain;
pub mod events;
pub mod listener;
pub mod tx_builder;
pub mod utils;
