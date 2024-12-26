//! This module defines the chain listener for 'solidity-ibc-eureka' contracts.

use alloy_primitives::{Address, TxHash};
use alloy_provider::Provider;
use alloy_rpc_types_eth::Filter;
use alloy_sol_types::SolEventInterface;
use alloy_transport::Transport;
use anyhow::{anyhow, Result};
use futures::future;
use ibc_eureka_solidity_types::ics26::router::{routerEvents, routerInstance};

use crate::{chain::EthEureka, events::EurekaEvent};

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

impl<T, P> ChainListener<T, P>
where
    T: Transport + Clone,
    P: Provider<T>,
{
    /// Get the chain ID.
    /// # Errors
    /// Returns an error if the chain ID cannot be fetched.
    pub async fn chain_id(&self) -> Result<String> {
        Ok(self
            .ics26_router
            .provider()
            .get_chain_id()
            .await?
            .to_string())
    }
}

#[async_trait::async_trait]
impl<T, P> ChainListenerService<EthEureka> for ChainListener<T, P>
where
    T: Transport + Clone,
    P: Provider<T>,
{
    async fn fetch_tx_events(&self, tx_ids: Vec<TxHash>) -> Result<Vec<EurekaEvent>> {
        Ok(
            future::try_join_all(tx_ids.into_iter().map(|tx_id| async move {
                let tx_height = self
                    .ics26_router
                    .provider()
                    .get_transaction_by_hash(tx_id)
                    .await?
                    .ok_or_else(|| anyhow!("Transaction {} not found", tx_id))?
                    .block_number
                    .ok_or_else(|| anyhow!("Transaction {} has not been mined", tx_id))?;

                let event_filter = Filter::new()
                    .events(EurekaEvent::evm_signatures())
                    .address(*self.ics26_router.address())
                    .from_block(tx_height)
                    .to_block(tx_height);

                Ok::<_, anyhow::Error>(
                    self.ics26_router
                        .provider()
                        .get_logs(&event_filter)
                        .await?
                        .iter()
                        .filter(|log| log.transaction_hash.unwrap_or_default() == tx_id)
                        .filter_map(|log| {
                            let sol_event = routerEvents::decode_log(&log.inner, true).ok()?.data;
                            EurekaEvent::try_from(sol_event).ok()
                        })
                        .collect::<Vec<_>>(),
                )
            }))
            .await?
            .into_iter()
            .flatten()
            .collect(),
        )
    }

    async fn fetch_events(&self, start_height: u64, end_height: u64) -> Result<Vec<EurekaEvent>> {
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
