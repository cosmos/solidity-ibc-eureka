use std::collections::HashMap;

use alloy_rpc_client::{ClientBuilder, ReqwestClient};

mod config;
mod header;

use attestor_packet_membership::Packets;
pub use config::OpConsensusClientConfig;

use crate::{
    adapter_client::{Adapter, AdapterError, UnsignedPacketAttestation, UnsignedStateAttestation},
    adapter_impls::optimism::header::SyncHeader,
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
    async fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> Result<UnsignedStateAttestation, AdapterError> {
        todo!()
    }
    async fn get_latest_unsigned_packet_attestation(
        &self,
        packets: &Packets,
    ) -> Result<UnsignedPacketAttestation, AdapterError> {
        todo!()
    }
}
