//! The `ChainSubmitter` submits txs to [`CosmosSdk`] based on events from [`CosmosSdk`].

use anyhow::Result;
use ibc_proto_eureka::ibc::core::channel::v2::{
    Channel, QueryChannelRequest, QueryChannelResponse,
};
use prost::Message;
//use sp1_ics07_tendermint_utils::rpc::TendermintRpcExt;
use tendermint_rpc::{Client, HttpClient};

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

    /// Fetches the eureka channel state from the target chain.
    /// # Errors
    /// Returns an error if the channel state cannot be fetched or decoded.
    pub async fn channel(&self, channel_id: String) -> Result<Channel> {
        let abci_resp = self
            .target_tm_client
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
