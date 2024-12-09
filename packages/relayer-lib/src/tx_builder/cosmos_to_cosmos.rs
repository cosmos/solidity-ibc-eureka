//! The `ChainSubmitter` submits txs to [`CosmosSdk`] based on events from [`CosmosSdk`].

use anyhow::Result;
use futures::future;
use ibc_proto_eureka::{
    ibc::{
        core::{
            channel::v2::{Channel, MsgTimeout, QueryChannelRequest, QueryChannelResponse},
            client::v1::Height,
        },
        lightclients::tendermint::v1::ClientState,
    },
    Protobuf,
};
use prost::Message;
use sp1_ics07_tendermint_utils::rpc::TendermintRpcExt;
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
        target_events: Vec<EurekaEvent>,
        target_channel_id: String,
    ) -> Result<Vec<u8>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let channel = self.channel(target_channel_id.clone()).await?;
        let client_state = ClientState::decode(
            self.target_tm_client
                .client_state(channel.client_id)
                .await?
                .value
                .as_slice(),
        )?;

        let light_block = self.source_tm_client.get_light_block(None).await?;
        let target_height = light_block.height().value().try_into()?;

        let _timeout_msgs = future::try_join_all(
            target_events
                .into_iter()
                .filter(|e| match e {
                    EurekaEvent::SendPacket(se) => {
                        now >= se.packet.timeoutTimestamp
                            && se.packet.sourceChannel == target_channel_id
                    }
                    _ => false,
                })
                .map(|e| async {
                    match e {
                        EurekaEvent::SendPacket(se) => {
                            let ibc_path = se.packet.receipt_commitment_path();
                            self.source_tm_client
                                .prove_path(&[b"ibc".to_vec(), ibc_path], target_height)
                                .await
                                .map(|(v, p)| {
                                    if v.is_empty() {
                                        Some(MsgTimeout {
                                            packet: Some(se.packet.into()),
                                            proof_unreceived: p.encode_vec(),
                                            proof_height: Some(Height {
                                                revision_number: client_state
                                                    .latest_height
                                                    .unwrap_or_default()
                                                    .revision_number,
                                                revision_height: target_height.into(),
                                            }),
                                            signer: String::new(),
                                        })
                                    } else {
                                        None
                                    }
                                })
                        }
                        _ => unreachable!(),
                    }
                }),
        )
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        todo!()
    }
}
