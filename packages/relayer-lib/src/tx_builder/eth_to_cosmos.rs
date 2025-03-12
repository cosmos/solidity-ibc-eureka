//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Ethereum.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alloy::{primitives::Address, providers::Provider};
use anyhow::Result;
use ethereum_apis::{beacon_api::client::BeaconApiClient, eth_api::client::EthApiClient};
use ethereum_light_client::consensus_state::ConsensusState;
use ethereum_light_client::header::{AccountUpdate, ActiveSyncCommittee, TrustedSyncCommittee};
use ethereum_light_client::test_utils::bls_verifier::fast_aggregate_verify;
use ethereum_light_client::{client_state::ClientState, header::Header};
use ethereum_types::consensus::domain::{compute_domain, DomainType};
use ethereum_types::consensus::light_client_header::{
    LightClientFinalityUpdate, LightClientUpdate,
};
use ethereum_types::consensus::signing_data::compute_signing_root;
use ethereum_types::consensus::slot::{compute_epoch_at_slot, compute_slot_at_timestamp};
use ethereum_types::consensus::sync_committee::SyncCommittee;
use ethereum_types::{
    consensus::sync_committee::compute_sync_committee_period_at_slot,
    execution::account_proof::AccountProof,
};
use ibc_eureka_solidity_types::ics26::router::routerInstance;
use ibc_proto_eureka::cosmos::tx::v1beta1::TxBody;
use ibc_proto_eureka::google::protobuf::Any;
use ibc_proto_eureka::ibc::core::client::v1::{Height, MsgUpdateClient};
use ibc_proto_eureka::ibc::lightclients::wasm::v1::ClientMessage;
use ibc_proto_eureka::ibc::lightclients::wasm::v1::{
    ClientState as WasmClientState, ConsensusState as WasmConsensusState,
};
use prost::Message;
use sp1_ics07_tendermint_utils::rpc::TendermintRpcExt;
use tendermint_rpc::{Client, HttpClient};

use crate::utils::{cosmos, wait_for_condition};
use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEventWithHeight,
};

use super::r#trait::TxBuilderService;

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from [`EthEureka`].
pub struct TxBuilder<P: Provider + Clone> {
    /// The ETH API client.
    pub eth_client: EthApiClient<P>,
    /// The Beacon API client.
    pub beacon_api_client: BeaconApiClient,
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<(), P>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
}

/// The `MockTxBuilder` produces txs to [`CosmosSdk`] based on events from [`EthEureka`]
/// for testing purposes.
pub struct MockTxBuilder<P: Provider + Clone> {
    /// The ETH API client.
    pub eth_client: EthApiClient<P>,
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<(), P>,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
}

impl<P: Provider + Clone> TxBuilder<P> {
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

    /// Fetch the Ethereum client state from the light client on cosmos.
    /// # Errors
    /// Returns an error if the client state cannot be fetched or decoded.
    pub async fn ethereum_client_state(&self, client_id: String) -> Result<ClientState> {
        let wasm_client_state_any = self.tm_client.client_state(client_id).await?;
        let wasm_client_state = WasmClientState::decode(wasm_client_state_any.value.as_slice())?;
        Ok(serde_json::from_slice(&wasm_client_state.data)?)
    }

    /// Fetches the Ethereum consensus state from the light client on cosmos.
    /// # Errors
    /// Returns an error if the consensus state cannot be fetched or decoded.
    pub async fn ethereum_consensus_state(
        &self,
        client_id: String,
        revision_height: u64,
    ) -> Result<ConsensusState> {
        // this was needed by the compiler to disambiguate the trait method call
        let wasm_consensus_state_any =
            sp1_ics07_tendermint_utils::rpc::TendermintRpcExt::consensus_state(
                &self.tm_client,
                client_id,
                revision_height,
            )
            .await?;
        let wasm_consensus_state =
            WasmConsensusState::decode(wasm_consensus_state_any.value.as_slice())
                .map_err(|e| anyhow::anyhow!("Failed to decode consensus state: {:?}", e))?;
        serde_json::from_slice(&wasm_consensus_state.data)
            .map_err(|e| anyhow::anyhow!("Failed to decode consensus state data: {:?}", e))
    }

    async fn get_sync_commitee_for_finalized_slot(
        &self,
        finalized_slot: u64,
    ) -> Result<SyncCommittee> {
        let block_root = self
            .beacon_api_client
            .beacon_block_root(&format!("{finalized_slot}"))
            .await?;

        let light_client_bootstrap = self
            .beacon_api_client
            .light_client_bootstrap(&block_root)
            .await?
            .data;

        let sync_committee = light_client_bootstrap.current_sync_committee;

        Ok(sync_committee)
    }

    async fn get_light_client_updates(
        &self,
        client_state: &ClientState,
        consensus_state: &ConsensusState,
        finality_update: LightClientFinalityUpdate,
    ) -> Result<Vec<LightClientUpdate>> {
        let trusted_period = compute_sync_committee_period_at_slot(
            client_state.slots_per_epoch,
            client_state.epochs_per_sync_committee_period,
            consensus_state.slot,
        );

        let target_period = compute_sync_committee_period_at_slot(
            client_state.slots_per_epoch,
            client_state.epochs_per_sync_committee_period,
            finality_update.finalized_header.beacon.slot,
        );

        tracing::debug!(
            "Getting light client updates from period {} to {}",
            trusted_period,
            target_period - trusted_period + 1
        );
        Ok(self
            .beacon_api_client
            .light_client_updates(trusted_period, target_period - trusted_period + 1) // + 1?
            .await?
            .into_iter()
            .map(|resp| resp.data)
            .collect::<Vec<_>>())
    }

    async fn wait_for_light_client_readiness(
        &self,
        target_block_number: u64,
    ) -> Result<LightClientFinalityUpdate> {
        // Shared mutable variable to store the latest update.
        let latest_update: Arc<Mutex<Option<LightClientFinalityUpdate>>> =
            Arc::new(Mutex::new(None));

        // Clone the Arc so the closure can access it.
        let latest_update_clone = latest_update.clone();
        wait_for_condition(
            Duration::from_secs(45 * 60),
            Duration::from_secs(10),
            || {
                // Clone again for the async move closure.
                let latest_update_inner = latest_update_clone.clone();
                async move {
                    tracing::debug!(
                        "Waiting for finality beyond target block number: {}",
                        target_block_number
                    );

                    let finality_update = self.beacon_api_client.finality_update().await?.data;
                    if finality_update.finalized_header.execution.block_number < target_block_number
                    {
                        tracing::info!(
                            "Finality not found: current finality execution block number: {}, Target execution block number: {}",
                            finality_update.finalized_header.execution.block_number,
                            target_block_number
                        );
                        return Ok(false);
                    }

                    // Store the current update.
                    *latest_update_inner.lock().unwrap() = Some(finality_update);
                    Ok(true)
                }
            },
        )
        .await?;

        // Retrieve the stored update.
        let stored_update = latest_update.lock().unwrap().take().ok_or_else(|| {
            anyhow::anyhow!("Finality update was not stored even though condition was met")
        })?;
        Ok(stored_update)
    }

    async fn light_client_update_to_header(
        &self,
        ethereum_client_state: ClientState,
        trusted_sync_committee: TrustedSyncCommittee,
        update: LightClientUpdate,
    ) -> Result<Header> {
        tracing::debug!(
            "Processing light client update for finalized slot {} ",
            update.finalized_header.beacon.slot,
        );

        let block_hex = format!("0x{:x}", update.finalized_header.execution.block_number);
        let ibc_contract_address: String = ethereum_client_state.ibc_contract_address.to_string();

        tracing::debug!("Getting account proof for execution block {}", block_hex);
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

        Ok(Header {
            trusted_sync_committee,
            account_update,
            consensus_update: update.clone(),
        })
    }
}

#[async_trait::async_trait]
impl<P> TxBuilderService<EthEureka, CosmosSdk> for TxBuilder<P>
where
    P: Provider + Clone,
{
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<EurekaEventWithHeight>,
        target_client_id: String,
    ) -> Result<Vec<u8>> {
        for event in &src_events {
            tracing::debug!("Source event: {:?}", event);
        }

        let latest_block_from_events = src_events
            .iter()
            .chain(dest_events.iter())
            .filter_map(|e| e.block_number)
            .max();

        let minimum_block_number = match latest_block_from_events {
            Some(latest_block) => latest_block,
            _ => self.eth_client.get_block_number().await?,
        };

        let target_height = Height {
            revision_number: 0,
            revision_height: minimum_block_number,
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let mut timeout_msgs = cosmos::target_events_to_timeout_msgs(
            dest_events,
            &target_client_id,
            &target_height,
            &self.signer_address,
            now,
        );

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &target_client_id,
            &target_height,
            &self.signer_address,
            now,
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        let ethereum_client_state = self.ethereum_client_state(target_client_id.clone()).await?;
        let ethereum_consensus_state = self
            .ethereum_consensus_state(target_client_id.clone(), 0)
            .await?;

        tracing::info!(
            "Relaying events from Ethereum to Cosmos for client {}, target block number: {}, client state latest slot: {}, consensus state slot: {}, current sync committee: {:?}, next sync committee: {:?}",
            target_client_id,
            minimum_block_number,
            ethereum_consensus_state.slot,
            ethereum_consensus_state.slot,
            ethereum_consensus_state.current_sync_committee,
            ethereum_consensus_state.next_sync_committee,
        );
        tracing::debug!("Target block number: {}", minimum_block_number);

        let finality_update = self
            .wait_for_light_client_readiness(minimum_block_number)
            .await?;

        let mut headers = vec![];

        let light_client_updates = self
            .get_light_client_updates(
                &ethereum_client_state,
                &ethereum_consensus_state,
                finality_update.clone(),
            )
            .await?;

        tracing::info!("light client updates: #{}", light_client_updates.len());

        let mut latest_trusted_slot = ethereum_consensus_state.slot;
        let mut latest_period = compute_sync_committee_period_at_slot(
            ethereum_client_state.slots_per_epoch,
            ethereum_client_state.epochs_per_sync_committee_period,
            latest_trusted_slot,
        );
        tracing::info!("Latest period: {}", latest_period);

        let mut current_next_sync_committee_agg_pubkey =
            ethereum_consensus_state.next_sync_committee;
        for update in &light_client_updates {
            tracing::debug!(
                "Processing light client update for finalized slot {} with trusted slot {}",
                update.finalized_header.beacon.slot,
                latest_trusted_slot
            );

            if update.finalized_header.beacon.slot <= latest_trusted_slot {
                tracing::info!(
                    "Skipping update for slot {}",
                    update.finalized_header.beacon.slot
                );
                continue;
            }

            let update_next_sync_committee_agg_pubkey = update
                .next_sync_committee
                .clone()
                .map(|sc| sc.aggregate_pubkey);

            // They are both options, so we can just compare them directly.
            if update_next_sync_committee_agg_pubkey == current_next_sync_committee_agg_pubkey {
                tracing::info!(
                    "Skipping header with same aggregate pubkey for slow {}",
                    update.finalized_header.beacon.slot
                );
                continue;
            }

            // TODO: Not sure
            let update_period = compute_sync_committee_period_at_slot(
                ethereum_client_state.slots_per_epoch,
                ethereum_client_state.epochs_per_sync_committee_period,
                update.finalized_header.beacon.slot,
            );
            if update_period == latest_period {
                tracing::info!(
                    "Skipping header with same period for slot {}",
                    update.finalized_header.beacon.slot
                );
                continue;
            }
            latest_period = update_period;

            let previous_next_sync_committee = self
                .get_sync_commitee_for_finalized_slot(update.finalized_header.beacon.slot)
                .await?;

            let trusted_sync_committee = TrustedSyncCommittee {
                trusted_slot: latest_trusted_slot,
                sync_committee: ActiveSyncCommittee::Next(previous_next_sync_committee),
            };
            let header = self
                .light_client_update_to_header(
                    ethereum_client_state.clone(),
                    trusted_sync_committee.clone(),
                    update.clone(),
                )
                .await?;
            headers.push(header.clone());

            tracing::info!(
                "Added header for slot from light client updates {}",
                update.finalized_header.beacon.slot,
            );
            tracing::debug!("Header: added {:?}", header);
            latest_trusted_slot = update.finalized_header.beacon.slot;
            current_next_sync_committee_agg_pubkey = update_next_sync_committee_agg_pubkey;
        }

        tracing::info!("Light client updates added to headers: #{}", headers.len());

        // If the latest header is earlier than the finality update, we need to add a header for the finality update.
        if headers.last().is_none_or(|last_header| {
            last_header.consensus_update.finalized_header.beacon.slot
                < finality_update.finalized_header.beacon.slot
        }) {
            let finality_update_sync_committee = self
                .get_sync_commitee_for_finalized_slot(finality_update.attested_header.beacon.slot)
                .await?;
            // TODO: Add asserts to make sure they are in the correct period
            //
            tracing::info!(
                "current sync committee used in finality update header: {:?}",
                finality_update_sync_committee.aggregate_pubkey
            );

            let trusted_sync_committee = TrustedSyncCommittee {
                trusted_slot: latest_trusted_slot,
                sync_committee: ActiveSyncCommittee::Current(
                    finality_update_sync_committee.clone(),
                ),
            };

            let participant_pubkeys = finality_update
                .sync_aggregate
                .sync_committee_bits
                .iter()
                .flat_map(|byte| (0..8).map(move |i| (byte & (1 << i)) != 0))
                .zip(finality_update_sync_committee.pubkeys.iter())
                .filter_map(|(included, pubkey)| included.then_some(*pubkey))
                .collect::<Vec<_>>();

            let fork_version_slot = std::cmp::max(finality_update.signature_slot, 1) - 1;
            let fork_version =
                ethereum_client_state
                    .fork_parameters
                    .compute_fork_version(compute_epoch_at_slot(
                        ethereum_client_state.slots_per_epoch,
                        fork_version_slot,
                    ));

            let domain = compute_domain(
                DomainType::SYNC_COMMITTEE,
                Some(fork_version),
                Some(ethereum_client_state.genesis_validators_root),
                ethereum_client_state.fork_parameters.genesis_fork_version,
            );
            tracing::info!("Domain: {:?}", domain);
            let signing_root =
                compute_signing_root(&finality_update.attested_header.beacon, domain);
            tracing::info!("Signing root: {:?}", signing_root);

            let res = fast_aggregate_verify(
                &participant_pubkeys,
                signing_root,
                finality_update.sync_aggregate.sync_committee_signature,
            );
            if res.is_err() {
                tracing::error!("Failed to verify sync committee signature: {:?}", res);
            }

            tracing::info!(
                "number participants: {}, number pubkeys: {}, num sync aggregate pubkeys: {}",
                participant_pubkeys.len(),
                finality_update_sync_committee.pubkeys.len(),
                finality_update.sync_aggregate.sync_committee_bits.len()
            );

            let header = self
                .light_client_update_to_header(
                    ethereum_client_state.clone(),
                    trusted_sync_committee.clone(),
                    finality_update.clone().into(),
                )
                .await?;
            headers.push(header.clone());
            latest_trusted_slot = finality_update.finalized_header.beacon.slot;
            tracing::info!(
                "Added header for slot from finality update {}: {}",
                finality_update.finalized_header.beacon.slot,
                serde_json::to_string(&header)?
            );
        }

        tracing::info!("Headers assembled: #{}", headers.len());

        let initial_period = compute_sync_committee_period_at_slot(
            ethereum_client_state.slots_per_epoch,
            ethereum_client_state.epochs_per_sync_committee_period,
            ethereum_consensus_state.slot,
        );
        let latest_period = compute_sync_committee_period_at_slot(
            ethereum_client_state.slots_per_epoch,
            ethereum_client_state.epochs_per_sync_committee_period,
            latest_trusted_slot,
        );
        tracing::info!(
            "Initial slot: {}, latest trusted slot: {}, initial period: {}, latest period: {}",
            ethereum_consensus_state.slot,
            latest_trusted_slot,
            initial_period,
            latest_period
        );

        let proof_block_number = headers
            .last()
            .map(|header| {
                header
                    .consensus_update
                    .finalized_header
                    .execution
                    .block_number
            })
            .unwrap();

        cosmos::inject_ethereum_proofs(
            &mut recv_msgs,
            &mut ack_msgs,
            &mut timeout_msgs,
            &self.eth_client,
            &ethereum_client_state.ibc_contract_address.to_string(),
            ethereum_client_state.ibc_commitment_slot,
            proof_block_number,
            latest_trusted_slot,
        )
        .await?;

        let update_msgs = headers
            .into_iter()
            .map(|header| -> Result<MsgUpdateClient> {
                let header_bz = serde_json::to_vec(&header)?;
                let client_msg = Any::from_msg(&ClientMessage { data: header_bz })?;
                Ok(MsgUpdateClient {
                    client_id: target_client_id.clone(),
                    client_message: Some(client_msg),
                    signer: self.signer_address.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let all_msgs = update_msgs
            .into_iter()
            .map(|m| Any::from_msg(&m))
            .chain(timeout_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(recv_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(ack_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .collect::<Result<Vec<_>, _>>()?;

        let tx_body = TxBody {
            messages: all_msgs,
            ..Default::default()
        };

        // Final check to make sure the target chain's calculated slot is greater than our latest
        // update slot:
        wait_for_condition(
            Duration::from_secs(15 * 60),
            Duration::from_secs(10),
            || async {
                let latests_tm_block = self.tm_client.latest_block().await?;
                let latest_onchain_timestamp = latests_tm_block.block.header.time.unix_timestamp();
                let calculated_slot = compute_slot_at_timestamp(
                    ethereum_client_state.genesis_time,
                    ethereum_client_state.genesis_slot,
                    ethereum_client_state.seconds_per_slot,
                    latest_onchain_timestamp.try_into().unwrap(),
                )
                .unwrap();
                tracing::debug!(
                    "Waiting for target chain to catch up to slot {}",
                    latest_trusted_slot
                );
                Ok(calculated_slot > finality_update.signature_slot)
            },
        )
        .await?;

        Ok(tx_body.encode_to_vec())
    }
}

impl<P: Provider + Clone> MockTxBuilder<P> {
    /// Create a new [`MockTxBuilder`] instance for testing.
    pub fn new(ics26_address: Address, provider: P, signer_address: String) -> Self {
        Self {
            eth_client: EthApiClient::new(provider.clone()),
            ics26_router: routerInstance::new(ics26_address, provider),
            signer_address,
        }
    }
}

#[async_trait::async_trait]
impl<P> TxBuilderService<EthEureka, CosmosSdk> for MockTxBuilder<P>
where
    P: Provider + Clone,
{
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<EurekaEventWithHeight>,
        target_client_id: String,
    ) -> Result<Vec<u8>> {
        let target_block_number = self.eth_client.get_block_number().await?;

        tracing::info!(
            "Relaying events from Ethereum to Cosmos for client {}",
            target_client_id
        );
        tracing::debug!("Target block number: {}", target_block_number);

        let target_height = Height {
            revision_number: 0,
            revision_height: target_block_number,
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let mut timeout_msgs = cosmos::target_events_to_timeout_msgs(
            dest_events,
            &target_client_id,
            &target_height,
            &self.signer_address,
            now,
        );

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &target_client_id,
            &target_height,
            &self.signer_address,
            now,
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        cosmos::inject_mock_proofs(&mut recv_msgs, &mut ack_msgs, &mut timeout_msgs);

        let all_msgs = timeout_msgs
            .into_iter()
            .map(|m| Any::from_msg(&m))
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
