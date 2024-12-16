//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from another Cosmos SDK chain.

use anyhow::Result;
use ibc_proto_eureka::{
    cosmos::tx::v1beta1::TxBody,
    google::protobuf::Any,
    ibc::{
        core::{
            channel::v2::{Channel, QueryChannelRequest, QueryChannelResponse},
            client::v1::MsgUpdateClient,
        },
        lightclients::tendermint::v1::ClientState,
    },
};
use prost::Message;
use sp1_ics07_tendermint_utils::{light_block::LightBlockExt, rpc::TendermintRpcExt};
use tendermint_rpc::{Client, HttpClient};

use crate::{
    chain::CosmosSdk,
    events::EurekaEvent,
    utils::cosmos::{src_events_to_recv_and_ack_msgs, target_events_to_timeout_msgs},
};

use super::r#trait::TxBuilderService;

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from [`CosmosSdk`].
#[allow(dead_code)]
pub struct TxBuilder {
    /// The HTTP client for the source chain.
    pub source_tm_client: HttpClient,
    /// The HTTP client for the target chain.
    pub target_tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub const fn new(
        source_tm_client: HttpClient,
        target_tm_client: HttpClient,
        signer_address: String,
    ) -> Self {
        Self {
            source_tm_client,
            target_tm_client,
            signer_address,
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
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        src_events: Vec<EurekaEvent>,
        target_events: Vec<EurekaEvent>,
        target_channel_id: String,
    ) -> Result<Vec<u8>> {
        let channel = self.channel(target_channel_id.clone()).await?;
        let client_state = ClientState::decode(
            self.target_tm_client
                .client_state(channel.client_id.clone())
                .await?
                .value
                .as_slice(),
        )?;

        let target_light_block = self.source_tm_client.get_light_block(None).await?;
        let target_height = target_light_block.height().value().try_into()?;
        let revision_number = client_state
            .latest_height
            .ok_or_else(|| anyhow::anyhow!("No latest height found"))?
            .revision_number;

        let timeout_msgs = target_events_to_timeout_msgs(
            target_events,
            &self.source_tm_client,
            &target_channel_id,
            revision_number,
            target_height,
            &self.signer_address,
        )
        .await?;

        let (recv_msgs, ack_msgs) = src_events_to_recv_and_ack_msgs(
            src_events,
            &self.source_tm_client,
            &target_channel_id,
            revision_number,
            target_height,
            &self.signer_address,
        )
        .await?;

        let trusted_light_block = self
            .source_tm_client
            .get_light_block(Some(
                client_state
                    .latest_height
                    .ok_or_else(|| anyhow::anyhow!("No latest height found"))?
                    .revision_height
                    .try_into()?,
            ))
            .await?;
        let proposed_header = target_light_block.into_header(&trusted_light_block);
        let update_msg = MsgUpdateClient {
            client_id: channel.client_id,
            client_message: Some(proposed_header.into()),
            signer: self.signer_address.clone(),
        };

        let all_msgs = std::iter::once(Any::from_msg(&update_msg))
            .chain(timeout_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(recv_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(ack_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .collect::<Result<Vec<_>, _>>()?;
        if all_msgs.len() == 1 {
            anyhow::bail!("No messages to relay to Cosmos");
        }

        tracing::debug!(
            "Messages to be relayed to Cosmos: {:?}",
            all_msgs[1..].to_vec()
        );

        let tx_body = TxBody {
            messages: all_msgs,
            ..Default::default()
        };
        Ok(tx_body.encode_to_vec())
    }
}
