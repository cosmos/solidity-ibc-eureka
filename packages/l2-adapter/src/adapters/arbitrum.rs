use alloy::{consensus::Header as EthHeader, eips::BlockNumberOrTag};
use alloy_network::Ethereum;
use alloy_provider::{Provider, RootProvider};

mod config;

pub use config::ArbitrumClientConfig;

use crate::{
    header::Header,
    l2_adapter_client::{L2Adapter, L2AdapterClientError},
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

    async fn get_block_by_number(
        &self,
        peek_kind: &PeekKind,
    ) -> Result<EthHeader, L2AdapterClientError> {
        let kind = match peek_kind {
            PeekKind::Finalized => BlockNumberOrTag::Finalized,
            PeekKind::Latest => BlockNumberOrTag::Latest,
        };

        let block = self
            .client
            .get_block_by_number(kind)
            .await
            .map_err(|e| L2AdapterClientError::FinalizedBlockError(e.to_string()))?
            .ok_or_else(|| {
                L2AdapterClientError::FinalizedBlockError(format!(
                    "no Arbitrum block of kind {} found",
                    kind.to_string()
                ))
            })?;

        Ok(block.header.into())
    }
}

impl L2Adapter for ArbitrumClient {
    async fn get_latest_finalized_block(&self) -> Result<Header, L2AdapterClientError> {
        let header = self.get_block_by_number(&PeekKind::Finalized).await?;

        Ok(Header::new(
            header.number,
            header.state_root,
            header.timestamp,
        ))
    }
    async fn get_latest_unfinalized_block(&self) -> Result<Header, L2AdapterClientError> {
        let header = self.get_block_by_number(&PeekKind::Latest).await?;

        Ok(Header::new(
            header.number,
            header.state_root,
            header.timestamp,
        ))
    }
}
