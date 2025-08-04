use std::collections::HashMap;

use alloy::{
    consensus::BlockHeader,
    eips::{BlockId, BlockNumberOrTag},
};
use alloy_network::Ethereum;
use alloy_primitives::{address, Address, FixedBytes};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_client::{ClientBuilder, ReqwestClient};

mod config;
mod header;

use attestor_packet_membership::Packets;
pub use config::OpClientConfig;
use futures::{stream::FuturesUnordered, StreamExt};
use ibc_eureka_solidity_types::ics26::{router::routerInstance, IICS26RouterMsgs::Packet};

use crate::{
    adapter_client::{Adapter, AdapterError, UnsignedPacketAttestation, UnsignedStateAttestation},
    adapter_impls::optimism::header::SyncHeader,
};

#[derive(Debug)]
pub struct OpClient {
    raw_client: ReqwestClient,
    client: RootProvider,
    router: routerInstance<RootProvider>,
}

impl OpClient {
    pub fn from_config(config: &OpClientConfig) -> Self {
        let raw_client: ReqwestClient = ClientBuilder::default().http(config.url.parse().unwrap());
        let client = RootProvider::<Ethereum>::new_http(config.url.parse().unwrap());

        let address = Address::parse_checksummed(&config.router_address, None).unwrap();
        let router = routerInstance::new(address.into(), client.clone());

        Self {
            raw_client,
            client,
            router,
        }
    }

    async fn get_latest_block_number(&self) -> Result<u64, AdapterError> {
        let sync_state: HashMap<String, SyncHeader> = self
            .raw_client
            .request_noparams("optimism_syncStatus")
            .await
            .map_err(|e| AdapterError::FinalizedBlockError(e.to_string()))?;

        sync_state
            .get("unsafe_l2")
            .ok_or(AdapterError::UnfinalizedBlockError("no unsafe l2".into()))
            .map(|block| block.height)
    }

    async fn get_timestamp_for_block_at_height(&self, height: u64) -> Result<u64, AdapterError> {
        self.client
            .get_block_by_number(BlockNumberOrTag::Number(height))
            .await
            .map_err(|e| AdapterError::FinalizedBlockError(e.to_string()))?
            .ok_or_else(|| {
                AdapterError::FinalizedBlockError(format!(
                    "no OP block of kind {} found",
                    BlockNumberOrTag::Latest
                ))
            })
            .map(|header| header.header.timestamp())
    }

    async fn get_historical_packet_commitment(
        &self,
        packet: &Packet,
        block_number: u64,
    ) -> Result<[u8; 32], AdapterError> {
        let fixed: FixedBytes<32> = packet.commitment_path().as_slice().try_into().unwrap();

        self.router
            .getCommitment(fixed)
            .block(BlockId::Number(BlockNumberOrTag::Number(block_number)))
            .call()
            .await
            .map_err(|e| {
                AdapterError::FinalizedBlockError(format!("Failed to get commitment: {}", e))
            })
            .map(|commitment| commitment.into())
    }
}

impl Adapter for OpClient {
    async fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> Result<UnsignedStateAttestation, AdapterError> {
        let ts = self.get_timestamp_for_block_at_height(height).await?;
        Ok(UnsignedStateAttestation {
            height,
            timestamp: ts,
        })
    }
    async fn get_latest_unsigned_packet_attestation(
        &self,
        packets: &Packets,
    ) -> Result<UnsignedPacketAttestation, AdapterError> {
        let mut futures = FuturesUnordered::new();
        let height = self.get_latest_block_number().await.unwrap();

        for p in packets.packets() {
            let packet: Packet = serde_json::from_slice(p).unwrap();
            let validate_commitment = async move |packet: Packet, height: u64| {
                let cmt = self
                    .get_historical_packet_commitment(&packet, height)
                    .await?;
                if packet.commitment() != cmt {
                    Err(AdapterError::FinalizedBlockError("something".into()))
                } else {
                    Ok(cmt)
                }
            };
            futures.push(validate_commitment(packet, height));
        }

        let mut validated = Vec::with_capacity(futures.len());
        while let Some(maybe_cmt) = futures.next().await {
            match maybe_cmt {
                Ok(cmt) => validated.push(cmt),
                Err(e) => {
                    tracing::error!(
                        "failed to retrieve packet commitment for due to {}",
                        e.to_string()
                    );
                }
            }
        }

        Ok(UnsignedPacketAttestation {
            height,
            packets: validated,
        })
    }
}
