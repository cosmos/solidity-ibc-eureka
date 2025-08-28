use std::future::Future;

use alloy::primitives::FixedBytes;
use ibc_eureka_relayer_lib::{
    events::EurekaEventWithHeight,
    listener::{cosmos_sdk, ChainListenerService},
};

pub struct TxAdapter([u8; 32]);

impl From<FixedBytes<32>> for TxAdapter {
    fn from(value: FixedBytes<32>) -> Self {
        Self(value.0)
    }
}

impl From<TxAdapter> for tendermint::Hash {
    fn from(value: TxAdapter) -> Self {
        Self::Sha256(value.0)
    }
}

pub trait TxListener: Send + Sync + 'static {
    fn fetch_tx_events(
        &self,
        tx_ids: Vec<TxAdapter>,
    ) -> impl Future<Output = Result<Vec<EurekaEventWithHeight>, anyhow::Error>> + Send;
}

impl TxListener for cosmos_sdk::ChainListener {
    async fn fetch_tx_events(
        &self,
        tx_ids: Vec<TxAdapter>,
    ) -> Result<Vec<EurekaEventWithHeight>, anyhow::Error> {
        ChainListenerService::fetch_tx_events(self, tx_ids.into_iter().map(Into::into).collect())
            .await
    }
}
