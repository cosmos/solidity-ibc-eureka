#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![doc = include_str!("../README.md")]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

pub mod consensus;
pub mod execution;
