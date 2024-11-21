//! This module defines the chain listener for 'ibc-go-eureka'.

use futures::{stream, StreamExt, TryStreamExt};
use tendermint::Hash;
use tendermint_rpc::{Client, HttpClient};

use anyhow::Result;

use crate::{chain::CosmosSdk, events::EurekaEvent};

use super::ChainListenerService;

#[async_trait::async_trait]
impl ChainListenerService<CosmosSdk> for HttpClient {
    async fn fetch_tx_events(&self, tx_id: Hash) -> Result<Vec<EurekaEvent>> {
        Ok(self
            .tx(tx_id, false)
            .await?
            .tx_result
            .events
            .into_iter()
            .filter_map(|e| EurekaEvent::try_from(e).ok())
            .collect())
    }

    async fn fetch_events(&self, start_height: u32, end_height: u32) -> Result<Vec<EurekaEvent>> {
        Ok(stream::iter(start_height..=end_height)
            .then(|h| async move { self.block_results(h).await })
            .try_fold(vec![], |mut acc, resp| async move {
                acc.extend(
                    resp.txs_results
                        .unwrap_or_default()
                        .into_iter()
                        .flat_map(|tx| tx.events)
                        .chain(resp.begin_block_events.unwrap_or_default())
                        .chain(resp.end_block_events.unwrap_or_default())
                        .chain(resp.finalize_block_events)
                        .filter_map(|e| EurekaEvent::try_from(e).ok()),
                );
                Ok::<_, tendermint_rpc::Error>(acc)
            })
            .await?)
    }
}
