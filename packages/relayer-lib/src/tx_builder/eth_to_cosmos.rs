//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Ethereum.

use std::time::Duration;

use alloy::{primitives::Address, providers::Provider, transports::Transport};
use anyhow::Result;
use ethereum_apis::{beacon_api::client::BeaconApiClient, eth_api::client::EthApiClient};
use ethereum_light_client::consensus_state::ConsensusState;
use ethereum_light_client::header::{AccountUpdate, TrustedSyncCommittee};
use ethereum_light_client::{client_state::ClientState, header::Header};
use ethereum_types::consensus::bls::BlsPublicKey;
use ethereum_types::consensus::light_client_header::LightClientUpdate;
use ethereum_types::consensus::slot::compute_slot_at_timestamp;
use ethereum_types::{
    consensus::sync_committee::compute_sync_committee_period_at_slot,
    execution::account_proof::AccountProof,
};
use ibc_eureka_solidity_types::ics26::router::routerInstance;
use ibc_proto_eureka::cosmos::tx::v1beta1::TxBody;
use ibc_proto_eureka::google::protobuf::Any;
use ibc_proto_eureka::ibc::core::client::v1::{Height, MsgUpdateClient};
use ibc_proto_eureka::ibc::lightclients::wasm::v1::ClientMessage;
use ibc_proto_eureka::ibc::{
    core::{
        channel::v2::{Channel, QueryChannelRequest, QueryChannelResponse},
        client::v1::{
            QueryClientStateRequest, QueryClientStateResponse, QueryConsensusStateRequest,
            QueryConsensusStateResponse,
        },
    },
    lightclients::wasm::v1::{
        ClientState as WasmClientState, ConsensusState as WasmConsensusState,
    },
};
use prost::Message;
use tendermint_rpc::{Client, HttpClient};

use crate::utils::{cosmos, wait_for_condition};
use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEvent,
};

use super::r#trait::TxBuilderService;

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from [`EthEureka`].
#[allow(dead_code)]
pub struct TxBuilder<T: Transport + Clone, P: Provider<T> + Clone> {
    /// The ETH API client.
    pub eth_client: EthApiClient<T, P>,
    /// The Beacon API client.
    pub beacon_api_client: BeaconApiClient,
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<T, P>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
}

impl<T: Transport + Clone, P: Provider<T> + Clone> TxBuilder<T, P> {
    /// Create a new [`TxBuilder`] instance.
    pub fn new(
        ics26_address: Address,
        provider: P,
        beacon_api_url: String,
        tm_client: HttpClient,
        signer_address: String,
    ) -> Self {
        Self {
            eth_client: EthApiClient::new(provider.clone()),
            beacon_api_client: BeaconApiClient::new(beacon_api_url),
            ics26_router: routerInstance::new(ics26_address, provider),
            tm_client,
            signer_address,
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

    /// Fetch the Etereum client state from the light client on cosmos.
    /// # Errors
    /// Returns an error if the client state cannot be fetched or decoded.
    pub async fn ethereum_client_state(&self, client_id: String) -> Result<ClientState> {
        let abci_resp = self
            .tm_client
            .abci_query(
                Some("/ibc.core.client.v1.Query/ClientState".to_string()),
                QueryClientStateRequest { client_id }.encode_to_vec(),
                None,
                false,
            )
            .await?;

        let resp = QueryClientStateResponse::decode(abci_resp.value.as_slice())?;
        let wasm_client_state_any = resp
            .client_state
            .ok_or_else(|| anyhow::anyhow!("No client state found"))?;
        let wasm_client_state = WasmClientState::decode(wasm_client_state_any.value.as_slice())?;
        Ok(serde_json::from_slice(&wasm_client_state.data)?)
    }

    /// Fetches the Etheruem consensus state from the light client on cosmos.
    /// # Errors
    /// Returns an error if the consensus state cannot be fetched or decoded.
    pub async fn ethereum_consensus_state(
        &self,
        client_id: String,
        revision_height: u64,
    ) -> Result<ConsensusState> {
        let abci_resp = self
            .tm_client
            .abci_query(
                Some("/ibc.core.client.v1.Query/ConsensusState".to_string()),
                QueryConsensusStateRequest {
                    client_id,
                    revision_number: 0,
                    revision_height,
                    latest_height: revision_height == 0,
                }
                .encode_to_vec(),
                None,
                false,
            )
            .await?;

        let resp = QueryConsensusStateResponse::decode(abci_resp.value.as_slice())?;
        let wasm_consensus_state_any = resp
            .consensus_state
            .ok_or_else(|| anyhow::anyhow!("No consensus state found"))?;
        let wasm_consensus_state =
            WasmConsensusState::decode(wasm_consensus_state_any.value.as_slice())
                .map_err(|e| anyhow::anyhow!("Failed to decode consensus state: {:?}", e))?;
        serde_json::from_slice(&wasm_consensus_state.data)
            .map_err(|e| anyhow::anyhow!("Failed to decode consensus state data: {:?}", e))
    }

    async fn get_light_client_updates(
        &self,
        client_state: &ClientState,
        consensus_state: &ConsensusState,
    ) -> Result<Vec<LightClientUpdate>> {
        let trusted_period = compute_sync_committee_period_at_slot(
            client_state.slots_per_epoch,
            client_state.epochs_per_sync_committee_period,
            consensus_state.slot,
        );

        let finality_update = self.beacon_api_client.finality_update().await?.data;

        let target_period = compute_sync_committee_period_at_slot(
            client_state.slots_per_epoch,
            client_state.epochs_per_sync_committee_period,
            finality_update.attested_header.beacon.slot,
        );
        Ok(self
            .beacon_api_client
            .light_client_updates(trusted_period + 1, target_period - trusted_period)
            .await?
            .into_iter()
            .map(|resp| resp.data)
            .collect::<Vec<_>>())
    }

    async fn wait_for_light_client_readiness(
        &self,
        target_block_number: u64,
        ethereum_client_state: &ClientState,
        ethereum_consensus_state: &ConsensusState,
    ) -> Result<()> {
        wait_for_condition(
            Duration::from_secs(60 * 10),
            Duration::from_secs(10),
            || async move {
                tracing::debug!("Waiting for finality and light client updates");

                let light_client_updates = self.get_light_client_updates(ethereum_client_state, ethereum_consensus_state).await?;

                let mut latest_light_client_update_block_number = 0;
                let mut latest_ligth_client_signature_slot = 0;
                for update in light_client_updates.as_slice() {
                    if update.attested_header.beacon.slot > latest_light_client_update_block_number {
                        latest_light_client_update_block_number =
                            update.attested_header.execution.block_number;
                    }
                    if update.signature_slot > latest_ligth_client_signature_slot {
                        latest_ligth_client_signature_slot = update.signature_slot;
                    }
                }

                let finality_update = self.beacon_api_client.finality_update().await?.data;
                let latest_finalized_block_number =
                    finality_update.attested_header.execution.block_number;

                let computed_slot = compute_slot_at_timestamp(
                    ethereum_client_state.genesis_time,
                    ethereum_client_state.seconds_per_slot,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs(),
                )
                .unwrap();
                tracing::debug!(
                    "Finality block number: {}, Update block number: {}, Update signature slot: {}, Target block number: {}, computed slot: {}",
                    latest_finalized_block_number,
                    latest_light_client_update_block_number,
                    latest_ligth_client_signature_slot,
                    target_block_number,
                    computed_slot,
                );
                if latest_finalized_block_number > target_block_number
                    && latest_light_client_update_block_number > target_block_number
                    //&& target_period > trusted_period
                    && computed_slot > latest_ligth_client_signature_slot
                {
                    return Ok(true);
                }
                Ok(false)
            },
        )
        .await?;

        Ok(())
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
        src_events: Vec<EurekaEvent>,
        dest_events: Vec<EurekaEvent>,
        target_channel_id: String,
    ) -> Result<Vec<u8>> {
        let target_block_number = self.eth_client.get_block_number().await?;
        let channel = self.channel(target_channel_id.clone()).await?;

        tracing::info!(
            "Relaying events from Ethereum to Cosmos for channel {}",
            target_channel_id
        );
        tracing::debug!("Target block number: {}", target_block_number);

        let ethereum_client_state = self
            .ethereum_client_state(channel.client_id.clone())
            .await?;
        let ethereum_consensus_state = self
            .ethereum_consensus_state(channel.client_id.clone(), 0)
            .await?;

        self.wait_for_light_client_readiness(
            target_block_number,
            &ethereum_client_state,
            &ethereum_consensus_state,
        )
        .await?;
        let light_client_updates = self
            .get_light_client_updates(&ethereum_client_state, &ethereum_consensus_state)
            .await?;
        tracing::debug!("light client updates: #{}", light_client_updates.len());

        let mut headers = vec![];
        let mut trusted_slot = ethereum_consensus_state.slot;
        let mut prev_pub_agg_key = BlsPublicKey::default();
        for update in &light_client_updates {
            tracing::debug!(
                "Processing light client update for slot {} with trusted slot {}",
                update.attested_header.beacon.slot,
                trusted_slot
            );

            let block_hex = format!("0x{:x}", update.attested_header.execution.block_number);
            let ibc_contract_address: String =
                ethereum_client_state.ibc_contract_address.to_string();

            tracing::debug!("Getting proof for block {}", block_hex);
            let proof = self
                .eth_client
                .get_proof(&ibc_contract_address, vec![], block_hex)
                .await?;

            let account_update = AccountUpdate {
                account_proof: AccountProof {
                    proof: proof.account_proof,
                    storage_root: proof.storage_hash,
                },
            };

            let mut previous_period = 0;
            let current_period = compute_sync_committee_period_at_slot(
                ethereum_client_state.slots_per_epoch,
                ethereum_client_state.epochs_per_sync_committee_period,
                update.attested_header.beacon.slot,
            );
            if current_period > 1 {
                previous_period = current_period - 1;
            }

            tracing::debug!("Getting updates for previous period: {}", previous_period);

            let previous_light_client_updates = self
                .beacon_api_client
                .light_client_updates(previous_period, 1)
                .await?
                .into_iter()
                .map(|resp| resp.data)
                .collect::<Vec<_>>();
            let previous_light_client_update = previous_light_client_updates.first().unwrap();
            let previous_next_sync_committee = previous_light_client_update
                .next_sync_committee
                .clone()
                .unwrap();
            if previous_next_sync_committee.aggregate_pubkey == prev_pub_agg_key {
                tracing::debug!("Skipping header with same aggregate pubkey");
                continue;
            }

            headers.push(Header {
                trusted_sync_committee: TrustedSyncCommittee {
                    trusted_slot,
                    sync_committee: ethereum_light_client::header::ActiveSyncCommittee::Next(
                        previous_next_sync_committee.clone(),
                    ),
                },
                account_update,
                consensus_update: update.clone(),
            });

            tracing::debug!(
                "Added header for slot {}",
                update.attested_header.beacon.slot
            );
            trusted_slot = update.attested_header.beacon.slot;
            prev_pub_agg_key = previous_next_sync_committee.aggregate_pubkey;
        }

        tracing::debug!("Headers assembled: #{}", headers.len());

        let update_msgs_iter = headers
            .iter()
            .map(|header| serde_json::to_vec(header).expect("Failed to serialize header"))
            .map(|header_bz| ClientMessage { data: header_bz })
            .map(|msg| Any::from_msg(&msg).expect("Failed to convert to Any"))
            .map(|client_msg| MsgUpdateClient {
                client_id: channel.client_id.clone(),
                client_message: Some(client_msg),
                signer: self.signer_address.clone(),
            })
            .map(|msg| Any::from_msg(&msg));

        let target_height = Height {
            revision_number: 0,
            revision_height: target_block_number,
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let mut timeout_msgs = cosmos::target_events_to_timeout_msgs(
            dest_events,
            &target_channel_id,
            &target_height,
            &self.signer_address,
            now,
        );

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &target_channel_id,
            &target_height,
            &self.signer_address,
            now,
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        cosmos::inject_ethereum_proofs(
            &mut recv_msgs,
            &mut ack_msgs,
            &mut timeout_msgs,
            &self.eth_client,
            &ethereum_client_state.ibc_contract_address.to_string(),
            ethereum_client_state.ibc_commitment_slot,
            target_block_number,
            trusted_slot,
        )
        .await?;

        let all_msgs = update_msgs_iter
            .chain(timeout_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(recv_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(ack_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .collect::<Result<Vec<_>, _>>()?;

        let tx_body = TxBody {
            messages: all_msgs,
            ..Default::default()
        };
        Ok(tx_body.encode_to_vec())
    }
}