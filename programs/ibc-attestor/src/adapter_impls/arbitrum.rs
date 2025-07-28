use alloy::{consensus::Header as EthHeader, eips::BlockNumberOrTag};
use alloy_network::Ethereum;
use alloy_provider::{Provider, RootProvider};

mod config;

use attestor_packet_membership::Packets;
pub use config::ArbitrumClientConfig;

use crate::adapter_client::{
    Adapter, AdapterError, UnsignedPacketAttestation, UnsignedStateAttestation,
};

/// Relevant chain peek options. For their Arbitrum
/// interpretation see [these docs](https://docs.arbitrum.io/for-devs/troubleshooting-building#how-many-block-numbers-must-we-wait-for-in-arbitrum-before-we-can-confidently-state-that-the-transaction-has-reached-finality)
enum PeekKind {
    /// Most recent confirmed L2 block on ETH L1
    Finalized,
    /// Latest L2 block
    Latest,
}

#[derive(Debug)]
pub struct ArbitrumClient {
    client: RootProvider,
}

impl ArbitrumClient {
    pub fn from_config(config: &ArbitrumClientConfig) -> Self {
        let client = RootProvider::<Ethereum>::new_http(config.url.parse().unwrap());
        Self { client }
    }

    async fn get_block_by_number(&self, peek_kind: &PeekKind) -> Result<EthHeader, AdapterError> {
        let kind = match peek_kind {
            PeekKind::Finalized => BlockNumberOrTag::Finalized,
            PeekKind::Latest => BlockNumberOrTag::Latest,
        };

        let block = self
            .client
            .get_block_by_number(kind)
            .await
            .map_err(|e| AdapterError::FinalizedBlockError(e.to_string()))?
            .ok_or_else(|| {
                AdapterError::FinalizedBlockError(format!("no Arbitrum block of kind {kind} found"))
            })?;

        Ok(block.header.into())
    }
}

impl Adapter for ArbitrumClient {
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
