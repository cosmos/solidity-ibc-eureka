use std::future::Future;

use alloy::{network::Ethereum, primitives::FixedBytes, providers::RootProvider};
use ibc_eureka_relayer_lib::{
    events::EurekaEventWithHeight,
    listener::{eth_eureka, ChainListenerService},
};

pub struct TxAdapter([u8; 32]);

impl From<FixedBytes<32>> for TxAdapter {
    fn from(value: FixedBytes<32>) -> Self {
        Self(value.0)
    }
}

impl From<TxAdapter> for FixedBytes<32> {
    fn from(value: TxAdapter) -> Self {
        value.0.into()
    }
}

impl From<[u8; 32]> for TxAdapter {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

pub trait TxListener: Send + Sync + 'static {
    fn fetch_tx_events(
        &self,
        tx_ids: Vec<TxAdapter>,
    ) -> impl Future<Output = Result<Vec<EurekaEventWithHeight>, anyhow::Error>> + Send;
}

impl TxListener for eth_eureka::ChainListener<RootProvider<Ethereum>> {
    async fn fetch_tx_events(
        &self,
        tx_ids: Vec<TxAdapter>,
    ) -> Result<Vec<EurekaEventWithHeight>, anyhow::Error> {
        ChainListenerService::fetch_tx_events(
            self,
            tx_ids.into_iter().map(|tx| tx.0.into()).collect(),
        )
        .await
    }
}
