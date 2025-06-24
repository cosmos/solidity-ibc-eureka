//! Defines Ethereum to Cosmos backwards compatible relayer module.
//! The module defined in [`eth_to_cosmos`](crate::modules::eth_to_cosmos) is not backwards
//! compatible with the CosmWasm Ethereum Light Client v1.2.0, so this module is provided
//! for backwards compatibility. And will be removed in the future.

use crate::api::relayer_service_server::RelayerService;

use super::eth_to_cosmos::EthToCosmosRelayerModule;
// use solidity_ibc_eureka_relayer_wasm_v1_2;

/// The `EthToCosmosRelayerCompatModule` struct defines the Ethereum to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct EthToCosmosCompatRelayerModule;

/// The `EthereumToCosmosRelayerCompatModuleService` defines the relayer service from Ethereum to Cosmos.
struct EthToCosmosRelayerCompatModuleService {
    new_service: Box<dyn RelayerService>,
    old_service: Box<dyn RelayerService>,
}

impl EthToCosmosRelayerCompatModuleService {
    /// Create a new `EthToCosmosRelayerCompatModuleService` instance.
    #[must_use]
    pub fn new(new_service: Box<dyn RelayerService>, old_service: Box<dyn RelayerService>) -> Self {
        Self {
            new_service,
            old_service,
        }
    }
}
