#![doc = include_str!("../README.md")]
#![deny(clippy::nursery, clippy::pedantic, warnings, missing_docs)]

/// Defines the API for the server generated by tonic.
#[allow(clippy::nursery, clippy::pedantic)]
pub mod api {
    tonic::include_proto!("relayer");
    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("relayer_descriptor");
}

pub mod cli;
pub mod core;
pub mod metrics;
pub mod modules;
