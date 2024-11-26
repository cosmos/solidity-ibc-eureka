//! Defines the shared trait that must be implemented by all relayer modules.

use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

use crate::api::relayer_service_server::RelayerService;

/// The `RelayerModule` trait defines the interface for a relayer module.
trait RelayerModule: RelayerService {
    type Config: Clone + Serialize + DeserializeOwned + Debug;

    fn new(config: Self::Config) -> Self;
}
