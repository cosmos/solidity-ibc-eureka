//! Defines the [`RelayerModule`] trait that must be implemented by all relayer modules.

use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

use crate::api::relayer_service_server::RelayerService;

/// The `RelayerModule` trait defines the interface for a relayer module.
#[tonic::async_trait]
pub trait RelayerModule: RelayerService {
    /// The configuration type for the relayer module.
    type Config: Clone + Serialize + DeserializeOwned + Debug;

    /// The name of the relayer module.
    /// This name is used to identify the module in the larger configuration file.
    const NAME: &'static str;

    /// Create a new instance of the relayer module.
    /// May panic if the configuration is invalid.
    async fn new(config: Self::Config) -> Self;
}
