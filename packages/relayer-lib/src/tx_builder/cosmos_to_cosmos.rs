//! The `ChainSubmitter` submits txs to [`CosmosSdk`] based on events from [`CosmosSdk`].

use anyhow::Result;
use tendermint_rpc::HttpClient;

use crate::{chain::CosmosSdk, events::EurekaEvent};

use super::r#trait::TxBuilderService;

/// The `TxBuilder` produces txs to [`EthEureka`] based on events from [`CosmosSdk`].
#[allow(dead_code)]
pub struct TxBuilder {
    /// The HTTP client for the source chain.
    pub source_tm_client: HttpClient,
    /// The HTTP client for the target chain.
    pub target_tm_client: HttpClient,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub const fn new(source_tm_client: HttpClient, target_tm_client: HttpClient) -> Self {
        Self {
            source_tm_client,
            target_tm_client,
        }
    }
}

#[async_trait::async_trait]
impl TxBuilderService<CosmosSdk, CosmosSdk> for TxBuilder {
    async fn relay_events(
        &self,
        _src_events: Vec<EurekaEvent>,
        _target_events: Vec<EurekaEvent>,
        _target_channel_id: String,
    ) -> Result<Vec<u8>> {
        todo!()
    }
}
