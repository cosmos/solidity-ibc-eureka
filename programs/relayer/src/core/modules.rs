//! Defines the [`RelayerModule`] trait that must be implemented by all relayer modules.

use std::marker::{Send, Sync};

/// The `RelayerModuleServer` trait defines the interface for launching a relayer module server.
#[tonic::async_trait]
pub trait ModuleServer: Send + Sync + 'static {
    /// Returns the name of the relayer module.
    fn name(&self) -> &'static str;
}
