//! Defines the [`RelayerModule`] trait that must be implemented by all relayer modules.

use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

use crate::api::relayer_service_server::RelayerService;

/// The `RelayerModule` trait defines the interface for a relayer module.
pub trait RelayerModule: RelayerService {
    /// The configuration type for the relayer module.
    type Config: Clone + Serialize + DeserializeOwned + Debug;

    /// Create a new instance of the relayer module.
    fn new(config: Self::Config) -> Self;
}
