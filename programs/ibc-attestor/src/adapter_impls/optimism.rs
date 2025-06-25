use std::collections::HashMap;

use alloy_rpc_client::{ClientBuilder, ReqwestClient};

mod config;
mod header;

pub use config::OpConsensusClientConfig;

use crate::{
    adapter_client::{Adapter, AdapterError},
    adapter_impls::optimism::header::SyncHeader,
    header::Header,
};

// Owned key type required by `Deserialize` macro
struct SyncState(HashMap<String, SyncHeader>);

#[derive(Debug)]
pub struct OpConsensusClient {
    client: ReqwestClient,
}

impl OpConsensusClient {
    pub fn from_config(config: &OpConsensusClientConfig) -> Self {
        let client: ReqwestClient = ClientBuilder::default().http(config.url.parse().unwrap());
        Self { client }
    }

    async fn get_sync_state(&self) -> Result<SyncState, AdapterError> {
        let sync_state: HashMap<String, SyncHeader> = self
            .client
            .request_noparams("optimism_syncStatus")
            .await
            .map_err(|e| AdapterError::FinalizedBlockError(e.to_string()))?;

        Ok(SyncState(sync_state))
    }
}

impl Adapter for OpConsensusClient {
    async fn get_latest_finalized_block(&self) -> Result<Header, AdapterError> {
        let state = self.get_sync_state().await?;

        state
            .0
            .get("finalized_l2")
            .map(|sync_header| {
                Header::new(sync_header.height, sync_header.hash, sync_header.timestamp)
            })
            .ok_or(AdapterError::FinalizedBlockError(
                "response received but no finalized L2 found in response".into(),
            ))
    }
    async fn get_latest_unfinalized_block(&self) -> Result<Header, AdapterError> {
        let state = self.get_sync_state().await?;

        state
            .0
            .get("unsafe_l2")
            .map(|sync_header| {
                Header::new(sync_header.height, sync_header.hash, sync_header.timestamp)
            })
            .ok_or(AdapterError::UnfinalizedBlockError(
                "response received but no unfinalized L2 found in response".into(),
            ))
    }
}
