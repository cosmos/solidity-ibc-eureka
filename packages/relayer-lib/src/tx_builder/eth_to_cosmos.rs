//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Ethereum.

use alloy::{primitives::Address, providers::Provider, transports::Transport};
use anyhow::Result;
use ibc_eureka_solidity_types::ics26::router::routerInstance;
use ibc_proto_eureka::ibc::core::channel::v2::{
    Channel, QueryChannelRequest, QueryChannelResponse,
};
use prost::Message;
use tendermint_rpc::{Client, HttpClient};

use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEvent,
};

use super::r#trait::TxBuilderService;

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from [`EthEureka`].
#[allow(dead_code)]
pub struct TxBuilder<T: Transport + Clone, P: Provider<T> + Clone> {
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<T, P>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
}

impl<T: Transport + Clone, P: Provider<T> + Clone> TxBuilder<T, P> {
    /// Create a new [`TxBuilder`] instance.
    pub const fn new(ics26_address: Address, provider: P, tm_client: HttpClient) -> Self {
        Self {
            ics26_router: routerInstance::new(ics26_address, provider),
            tm_client,
        }
    }

    /// Fetches the eureka channel state from the target chain.
    /// # Errors
    /// Returns an error if the channel state cannot be fetched or decoded.
    pub async fn channel(&self, channel_id: String) -> Result<Channel> {
        let abci_resp = self
            .tm_client
            .abci_query(
                Some("/ibc.core.channel.v2.Query/Channel".to_string()),
                QueryChannelRequest { channel_id }.encode_to_vec(),
                None,
                false,
            )
            .await?;

        QueryChannelResponse::decode(abci_resp.value.as_slice())?
            .channel
            .ok_or_else(|| anyhow::anyhow!("No channel state found"))
    }
}

#[async_trait::async_trait]
impl<T, P> TxBuilderService<EthEureka, CosmosSdk> for TxBuilder<T, P>
where
    T: Transport + Clone,
    P: Provider<T> + Clone,
{
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        _src_events: Vec<EurekaEvent>,
        _dest_events: Vec<EurekaEvent>,
        target_channel_id: String,
    ) -> Result<Vec<u8>> {
        let _channel = self.channel(target_channel_id.clone()).await?;
        Ok(vec![])
    }
}
