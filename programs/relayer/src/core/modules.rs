//! Defines the [`RelayerModule`] trait that must be implemented by all relayer modules.

use std::marker::{Send, Sync};

use anyhow::Result;

use crate::api::relayer_service_server::RelayerService;

/// The `RelayerModuleServer` trait defines the interface for launching a relayer module server.
#[tonic::async_trait]
pub trait ModuleServer: Send + Sync + 'static {
    /// Returns the name of the relayer module.
    fn name(&self) -> &'static str;

    /// Serve the relayer module RPC on the given address.
    async fn serve(&self, config: serde_json::Value) -> Result<Box<dyn RelayerService>>;
}
