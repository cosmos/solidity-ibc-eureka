use alloy::{
    consensus::BlockHeader,
    eips::{BlockId, BlockNumberOrTag},
    hex::FromHex,
    sol_types::SolValue,
};
use alloy_network::Ethereum;
use alloy_primitives::{keccak256, Address, FixedBytes};
use alloy_provider::{Provider, RootProvider};

mod config;

use attestor_packet_membership::Packets;
pub use config::OpClientConfig;
use futures::{stream::FuturesUnordered, StreamExt};
use ibc_eureka_solidity_types::ics26::{router::routerInstance, IICS26RouterMsgs::Packet};

use crate::adapter_client::{
    Adapter, AdapterError, UnsignedPacketAttestation, UnsignedStateAttestation,
};

#[derive(Debug)]
pub struct OpClient {
    client: RootProvider,
    router: routerInstance<RootProvider>,
}

impl OpClient {
    pub fn from_config(config: &OpClientConfig) -> Self {
        let client = RootProvider::<Ethereum>::new_http(config.url.parse().unwrap());

        let address = Address::from_hex(&config.router_address).unwrap();
        let router = routerInstance::new(address.into(), client.clone());

        Self { client, router }
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
        hashed_path: FixedBytes<32>,
        block_number: u64,
    ) -> Result<[u8; 32], AdapterError> {
        let cmt = self
            .router
            .getCommitment(hashed_path)
            .block(BlockId::Number(BlockNumberOrTag::Number(block_number)))
            .call()
            .await
            .map_err(|e| {
                AdapterError::FinalizedBlockError(format!("Failed to get commitment: {}", e))
            })?;

        // Array of 0s means not found
        let is_empty = cmt.iter().max() == Some(&0);
        if is_empty {
            Err(AdapterError::FinalizedBlockError(format!(
                "commitment path {:?} at height {block_number} not found",
                hashed_path
            )))
        } else {
            Ok(*cmt)
        }
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
    async fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &Packets,
        height: u64,
    ) -> Result<UnsignedPacketAttestation, AdapterError> {
        let mut futures = FuturesUnordered::new();

        for p in packets.packets() {
            let packet = Packet::abi_decode(p).unwrap();
            let validate_commitment = async move |packet: Packet, height: u64| {
                let commitment_path = packet.commitment_path();
                let hashed = keccak256(&commitment_path);
                let cmt = self
                    .get_historical_packet_commitment(hashed, height)
                    .await?;

                if &packet.commitment() != &cmt {
                    Err(AdapterError::FinalizedBlockError(format!(
                        "hashed paths are not the same: hashed {:?}, received {:?}",
                        *hashed, cmt
                    )))
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
