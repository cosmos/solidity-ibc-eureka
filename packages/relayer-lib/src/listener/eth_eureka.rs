//! This module defines the chain listener for 'solidity-ibc-eureka' contracts.

use alloy::{
    primitives::{Address, TxHash},
    providers::Provider,
    rpc::types::Filter,
    sol_types::SolEventInterface,
    transports::Transport,
};
use anyhow::{anyhow, Result};
use ibc_eureka_solidity_types::ics26::router::{routerEvents, routerInstance};

use crate::events::EurekaEvent;

use super::ChainListenerService;

/// The `ChainListenerService` listens for events on the Ethereum chain.
pub struct ChainListener<T: Transport + Clone, P: Provider<T>> {
    /// The IBC Eureka router instance.
    ics26_router: routerInstance<T, P>,
}

impl<T: Transport + Clone, P: Provider<T>> ChainListener<T, P> {
    /// Create a new `ChainListenerService` instance.
    pub const fn new(ics26_address: Address, provider: P) -> Self {
        Self {
            ics26_router: routerInstance::new(ics26_address, provider),
        }
    }
}

#[async_trait::async_trait]
impl<T, P> ChainListenerService for ChainListener<T, P>
where
    T: Transport + Clone,
    P: Provider<T>,
{
    type Event = EurekaEvent;
    type TxId = TxHash;
    type Height = u64;

    async fn fetch_tx_events(&self, tx_id: &Self::TxId) -> Result<Vec<Self::Event>> {
        let tx_height = self
            .ics26_router
            .provider()
            .get_transaction_by_hash(*tx_id)
            .await?
            .ok_or_else(|| anyhow!("Transaction {} not found", tx_id))?
            .block_number
            .ok_or_else(|| anyhow!("Transaction {} has not been mined", tx_id))?;
        let event_filter = Filter::new()
            .events(EurekaEvent::evm_signatures())
            .address(*self.ics26_router.address())
            .from_block(tx_height)
            .to_block(tx_height);

        Ok(self
            .ics26_router
            .provider()
            .get_logs(&event_filter)
            .await?
            .iter()
            .filter(|log| log.transaction_hash.unwrap_or_default() == *tx_id)
            .filter_map(|log| {
                let sol_event = routerEvents::decode_log(&log.inner, true).ok()?.data;
                EurekaEvent::try_from(sol_event).ok()
            })
            .collect())
    }

    async fn fetch_events(
        &self,
        start_height: Self::Height,
        end_height: Self::Height,
    ) -> Result<Vec<Self::Event>> {
        let event_filter = Filter::new()
            .events(EurekaEvent::evm_signatures())
            .address(*self.ics26_router.address())
            .from_block(start_height)
            .to_block(end_height);

        Ok(self
            .ics26_router
            .provider()
            .get_logs(&event_filter)
            .await?
            .iter()
            .filter_map(|log| {
                let sol_event = routerEvents::decode_log(&log.inner, true).ok()?.data;
                EurekaEvent::try_from(sol_event).ok()
            })
            .collect())
    }
}
