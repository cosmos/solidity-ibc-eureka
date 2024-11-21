//! The `ChainSubmitter` submits txs to [`EthEureka`] based on events from [`CosmosSdk`].

use alloy::{
    primitives::{Address, TxHash},
    providers::Provider,
    transports::Transport,
};
use anyhow::Result;
use ibc_eureka_solidity_types::ics26::router::routerInstance;
use tendermint_rpc::HttpClient;

use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEvent,
};

use super::r#trait::ChainSubmitterService;

/// The `ChainSubmitter` submits txs to [`EthEureka`] based on events from [`CosmosSdk`].
#[allow(dead_code)]
pub struct ChainSubmitter<T: Transport + Clone, P: Provider<T>> {
    /// The IBC Eureka router instance.
    ics26_router: routerInstance<T, P>,
    /// The HTTP client for the Cosmos SDK.
    tm_client: HttpClient,
}

impl<T: Transport + Clone, P: Provider<T>> ChainSubmitter<T, P> {
    /// Create a new `ChainListenerService` instance.
    pub const fn new(ics26_address: Address, provider: P, tm_client: HttpClient) -> Self {
        Self {
            ics26_router: routerInstance::new(ics26_address, provider),
            tm_client,
        }
    }
}

#[async_trait::async_trait]
impl<T, P> ChainSubmitterService<EthEureka, CosmosSdk> for ChainSubmitter<T, P>
where
    T: Transport + Clone,
    P: Provider<T>,
{
    async fn submit_events(&self, _events: Vec<EurekaEvent>) -> Result<TxHash> {
        todo!()
    }
}
