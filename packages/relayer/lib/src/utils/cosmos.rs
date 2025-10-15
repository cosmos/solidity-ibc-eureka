//! Relayer utilities for `CosmosSDK` chains.

use std::collections::HashMap;
use std::hash::BuildHasher;

use alloy::{hex, primitives::U256, providers::Provider};
use anyhow::Result;
use ethereum_apis::{beacon_api::client::BeaconApiClient, eth_api::client::EthApiClient};
use ethereum_light_client::membership::{evm_ics26_commitment_path, MembershipProof};
use ethereum_types::execution::{account_proof::AccountProof, storage_proof::StorageProof};
use futures::future;
use ibc_client_tendermint_types::ConsensusState;
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;
use ibc_eureka_utils::{light_block::LightBlockExt as _, rpc::TendermintRpcExt};
use ibc_proto_eureka::google::protobuf::Duration;
use ibc_proto_eureka::{
    cosmos::tx::v1beta1::TxBody,
    google::protobuf::Any,
    ibc::{
        core::{
            channel::v2::{Acknowledgement, MsgAcknowledgement, MsgRecvPacket, MsgTimeout},
            client::v1::{Height, MsgCreateClient, MsgUpdateClient},
        },
        lightclients::tendermint::v1::ClientState,
        lightclients::wasm::v1::{
            ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        },
    },
    Protobuf,
};
use prost::Message;
use tendermint_rpc::HttpClient;

use crate::{
    events::{EurekaEvent, EurekaEventWithHeight},
    tendermint_client::build_tendermint_client_state,
};

/// The key for the checksum hex in the tendermint create client parameters map.
pub const CHECKSUM_HEX: &str = "checksum_hex";

/// Converts a list of [`EurekaEvent`]s to a list of [`MsgTimeout`]s.
///
/// # Arguments
/// - `target_events` - The list of target events.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `dst_packet_seqs` - The list of dest packet sequences to filter. If empty, no filtering.
/// - `signer_address` - The signer address.
/// - `now` - The current time.
#[must_use]
pub fn target_events_to_timeout_msgs(
    target_events: Vec<EurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    dst_packet_seqs: &[u64],
    signer_address: &str,
    now: u64,
) -> Vec<MsgTimeout> {
    target_events
        .into_iter()
        .filter_map(|e| match e.event {
            EurekaEvent::SendPacket(packet) => (now >= packet.timeoutTimestamp
                && packet.sourceClient == dst_client_id
                && packet.destClient == src_client_id
                && (dst_packet_seqs.is_empty() || dst_packet_seqs.contains(&packet.sequence)))
            .then_some(MsgTimeout {
                packet: Some(packet.into()),
                proof_height: None,
                proof_unreceived: vec![],
                signer: signer_address.to_string(),
            }),
            EurekaEvent::WriteAcknowledgement(..) => None,
        })
        .collect()
}

/// Converts a list of [`EurekaEvent`]s to a list of [`MsgRecvPacket`]s and
/// [`MsgAcknowledgement`]s.
///
/// # Arguments
/// - `src_events` - The list of source events.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `src_packet_seqs` - The list of source packet sequences to filter. If empty, no filtering.
/// - `dst_packet_seqs` - The list of dest packet sequences to filter. If empty, no filtering.
/// - `signer_address` - The signer address.
/// - `now` - The current time.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn src_events_to_recv_and_ack_msgs(
    src_events: Vec<EurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    src_packet_seqs: &[u64],
    dst_packet_seqs: &[u64],
    signer_address: &str,
    now: u64,
) -> (Vec<MsgRecvPacket>, Vec<MsgAcknowledgement>) {
    let (src_send_events, src_ack_events): (Vec<_>, Vec<_>) = src_events
        .into_iter()
        .filter(|e| match &e.event {
            EurekaEvent::SendPacket(packet) => {
                packet.timeoutTimestamp > now
                    && packet.sourceClient == src_client_id
                    && packet.destClient == dst_client_id
                    && (src_packet_seqs.is_empty() || src_packet_seqs.contains(&packet.sequence))
            }
            EurekaEvent::WriteAcknowledgement(packet, _) => {
                packet.sourceClient == dst_client_id
                    && packet.destClient == src_client_id
                    && (dst_packet_seqs.is_empty() || dst_packet_seqs.contains(&packet.sequence))
            }
        })
        .partition(|e| match e.event {
            EurekaEvent::SendPacket(_) => true,
            EurekaEvent::WriteAcknowledgement(..) => false,
        });

    let recv_msgs = src_send_events
        .into_iter()
        .map(|e| match e.event {
            EurekaEvent::SendPacket(packet) => MsgRecvPacket {
                packet: Some(packet.into()),
                proof_height: None,
                proof_commitment: vec![],
                signer: signer_address.to_string(),
            },
            EurekaEvent::WriteAcknowledgement(..) => unreachable!(),
        })
        .collect::<Vec<MsgRecvPacket>>();

    let ack_msgs = src_ack_events
        .into_iter()
        .map(|e| match e.event {
            EurekaEvent::WriteAcknowledgement(packet, acks) => MsgAcknowledgement {
                packet: Some(packet.into()),
                acknowledgement: Some(Acknowledgement {
                    app_acknowledgements: acks.into_iter().map(Into::into).collect(),
                }),
                proof_height: None,
                proof_acked: vec![],
                signer: signer_address.to_string(),
            },
            EurekaEvent::SendPacket(_) => unreachable!(),
        })
        .collect::<Vec<MsgAcknowledgement>>();

    (recv_msgs, ack_msgs)
}

/// Parameters for updating a Tendermint IBC light client.
pub struct TmUpdateClientParams {
    /// Height the client will be updated to (proof height for packet verification).
    pub target_height: u64,
    /// Current trusted height of the client.
    pub trusted_height: u64,
    /// Header proving valid transition from `trusted_height` to `target_height`.
    pub proposed_header: ibc_proto::ibc::lightclients::tendermint::v1::Header,
}

/// Generates a Tendermint header for IBC client update from trusted height to height (latest if
/// omitted).
///
/// # Errors
/// - Failed light block retrieval from Tendermint node
pub async fn tm_update_client_params(
    trusted_height: u64,
    src_tm_client: &HttpClient,
    target_height: Option<u64>,
) -> anyhow::Result<TmUpdateClientParams> {
    let target_light_block = src_tm_client.get_light_block(target_height).await?;
    let target_height = target_light_block.signed_header.header.height.value();

    let trusted_light_block = src_tm_client.get_light_block(Some(trusted_height)).await?;

    tracing::info!(
        "Generating header to update from height: {} to height: {}",
        trusted_light_block.height().value(),
        target_light_block.height().value()
    );

    let proposed_header = target_light_block.into_header(&trusted_light_block);

    Ok(TmUpdateClientParams {
        target_height,
        trusted_height,
        proposed_header,
    })
}

/// Parameters for creating a new Tendermint IBC light client.
pub struct TmCreateClientParams {
    /// Latest height
    pub latest_height: u64,
    /// Consensus state
    pub client_state: ClientState,
    /// Initial trusted consensus state
    pub consensus_state: ConsensusState,
}

/// Generates parameters for creating a new Tendermint IBC light client.
/// # Arguments
/// * `src_tm_client` - HTTP client connected to the source Tendermint chain
///
/// # Returns
/// Client creation parameters with
///
/// # Errors
/// - Missing unbonding time in staking parameters
/// - Failed to fetch light block or chain parameters
pub async fn tm_create_client_params(
    src_tm_client: &HttpClient,
) -> anyhow::Result<TmCreateClientParams> {
    let latest_light_block = src_tm_client.get_light_block(None).await?;
    // NOTE: might cache
    let chain_id = latest_light_block.chain_id()?;

    let latest_height = latest_light_block.height().value();

    tracing::info!("Creating client at height: {latest_height}",);

    let height = Height {
        revision_number: chain_id.revision_number(),
        revision_height: latest_light_block.height().value(),
    };
    let unbonding_period = src_tm_client
        .sdk_staking_params()
        .await?
        .unbonding_time
        .ok_or_else(|| anyhow::anyhow!("No unbonding time found"))?;

    // Defaults to the recommended 2/3 of the UnbondingPeriod
    let trusting_period = Duration {
        seconds: 2 * (unbonding_period.seconds / 3),
        nanos: 0,
    };

    let client_state = build_tendermint_client_state(
        chain_id.to_string(),
        height,
        trusting_period,
        unbonding_period,
        vec![ics23::iavl_spec(), ics23::tendermint_spec()],
    );

    let consensus_state = latest_light_block.to_consensus_state();
    Ok(TmCreateClientParams {
        latest_height,
        client_state,
        consensus_state,
    })
}

/// Creates a Cosmos transaction for creating an IBC client with WASM light client support.
///
/// # Arguments
/// * `parameters` - Must contain `checksum_hex` key with the WASM code checksum
/// * `client_state_bytes` - Serialized client state data
/// * `consensus_state` - Serialized consensus state data
/// * `height` - Height parameter (currently unused in implementation)
/// * `signer_address` - Address of the transaction signer
///
/// # Returns
/// Encoded transaction body bytes
///
/// # Errors
/// * If parameters contains keys other than `checksum_hex`
/// * If `checksum_hex` parameter is missing
/// * If checksum hex decoding fails
pub fn cosmos_create_client_tx<H: BuildHasher>(
    parameters: &HashMap<String, String, H>,
    client_state_bytes: Vec<u8>,
    consensus_state: &WasmConsensusState,
    height: Height,
    signer_address: String,
) -> Result<Vec<u8>> {
    parameters
        .keys()
        .find(|k| k.as_str() != CHECKSUM_HEX)
        .map_or(Ok(()), |param| {
            Err(anyhow::anyhow!(
                "Unexpected parameter: `{param}`, only `{CHECKSUM_HEX}` is allowed"
            ))
        })?;
    let checksum = hex::decode(
        parameters
            .get(CHECKSUM_HEX)
            .ok_or_else(|| anyhow::anyhow!("Missing `{CHECKSUM_HEX}` parameter"))?,
    )?;

    let client_state = WasmClientState {
        data: client_state_bytes,
        checksum,
        latest_height: Some(height),
    };

    let msg = MsgCreateClient {
        client_state: Some(Any::from_msg(&client_state)?),
        consensus_state: Some(Any::from_msg(consensus_state)?),
        signer: signer_address,
    };

    Ok(TxBody {
        messages: vec![Any::from_msg(&msg)?],
        ..Default::default()
    }
    .encode_to_vec())
}

/// Creates a Cosmos transaction for updating an IBC client with a new consensus state.
///
/// # Arguments
/// * `dst_client_id` - The identifier of the client to update
/// * `consensus_state` - The consensus state of update
/// * `signer_address` - Address of the transaction signer
///
/// # Returns
/// Encoded transaction body bytes ready for signing and submission
///
/// # Errors
/// * If message encoding fails
pub fn cosmos_update_client_tx(
    dst_client_id: String,
    consensus_state: &WasmConsensusState,
    signer_address: String,
) -> Result<Vec<u8>> {
    tracing::info!(
        "Generating tx to update light client on cosmos: {}",
        dst_client_id
    );

    let msg = MsgUpdateClient {
        client_id: dst_client_id,
        client_message: Some(Any::from_msg(consensus_state)?),
        signer: signer_address,
    };

    Ok(TxBody {
        messages: vec![Any::from_msg(&msg)?],
        ..Default::default()
    }
    .encode_to_vec())
}
/// Fetches the latest Tendermint height from the source chain while preserving the revision number.
///
/// # Arguments
/// * `client_state` - The IBC client state containing the current revision number
/// * `source_tm_client` - HTTP client for querying the source Tendermint chain
///
/// # Returns
/// * `Height` with the latest block height from source chain and preserved revision number
///
/// # Errors
/// * If the light block cannot be fetched from the source chain
/// * If the client state has no latest height set
pub async fn get_latest_tm_heigth(
    client_state: ClientState,
    source_tm_client: &HttpClient,
) -> Result<Height> {
    let target_light_block = source_tm_client.get_light_block(None).await?;
    let revision_height = target_light_block.height().value();
    let revision_number = client_state
        .latest_height
        .ok_or_else(|| anyhow::anyhow!("No latest height found"))?
        .revision_number;

    let latest_height = Height {
        revision_number,
        revision_height,
    };

    Ok(latest_height)
}

/// Generates and injects tendermint proofs for rec, ack and timeout messages.
/// # Errors
/// Returns an error a proof cannot be generated for any of the provided messages.
/// # Panics
/// Panics if the provided messages do not contain a valid packet.
pub async fn inject_tendermint_proofs(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
    source_tm_client: &HttpClient,
    target_height: &Height,
) -> Result<()> {
    future::try_join_all(recv_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let commitment_path = packet.commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(
                &[b"ibc".to_vec(), commitment_path],
                target_height.revision_height,
            )
            .await?;
        if value.is_empty() {
            anyhow::bail!("Membership value is empty")
        }

        msg.proof_commitment = proof.encode_vec();
        msg.proof_height = Some(*target_height);
        anyhow::Ok(())
    }))
    .await?;

    future::try_join_all(ack_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let ack_path = packet.ack_commitment_path();

        tracing::info!("=== GENERATING ACK PROOF ===");
        tracing::info!("  Packet sequence: {}", packet.sequence);
        tracing::info!(
            "  From: {} -> To: {}",
            packet.sourceClient,
            packet.destClient
        );
        tracing::info!(
            "  Target height for proof: {}",
            target_height.revision_height
        );
        tracing::info!(
            "  prove_path will query Cosmos at: {} (target - 1)",
            target_height.revision_height - 1
        );
        tracing::info!(
            "  Proof will verify against app_hash from height: {}",
            target_height.revision_height
        );

        // Log the exact path components
        let path_component_0 = b"ibc".to_vec();
        let path_component_1 = ack_path.clone();
        tracing::info!(
            "  Path component [0] (string): {}",
            String::from_utf8_lossy(&path_component_0)
        );
        tracing::info!(
            "  Path component [0] (hex): {}",
            hex::encode(&path_component_0)
        );
        tracing::info!(
            "  Path component [1] (hex): {}",
            hex::encode(&path_component_1)
        );
        tracing::info!(
            "  Path component [1] length: {} bytes",
            path_component_1.len()
        );

        // Query Cosmos to get the app_hash at target height
        let target_light_block = source_tm_client
            .get_light_block(Some(target_height.revision_height))
            .await?;
        let cosmos_app_hash = target_light_block.signed_header.header.app_hash;
        tracing::info!(
            "  Cosmos app_hash at height {}: {:?}",
            target_height.revision_height,
            cosmos_app_hash.as_bytes()
        );
        tracing::info!(
            "  Cosmos app_hash (hex): {}",
            hex::encode(cosmos_app_hash.as_bytes())
        );

        let (value, proof) = source_tm_client
            .prove_path(
                &[path_component_0, path_component_1],
                target_height.revision_height,
            )
            .await?;

        tracing::info!("=== ACK PROOF GENERATED ===");
        tracing::info!("  Value length: {} bytes", value.len());
        tracing::info!("  Value (ack commitment, hex): {}", hex::encode(&value));
        tracing::info!("  Proof ops count: {}", proof.proofs.len());
        tracing::info!(
            "  Setting msg.proof_height = {}",
            target_height.revision_height
        );

        if value.is_empty() {
            anyhow::bail!(
                "Membership value is empty at height {}",
                target_height.revision_height
            )
        }

        msg.proof_acked = proof.encode_vec();
        msg.proof_height = Some(*target_height);

        tracing::info!("  Proof encoded size: {} bytes", msg.proof_acked.len());
        tracing::info!("  Final proof_height in message: {:?}", msg.proof_height);

        anyhow::Ok(())
    }))
    .await?;

    future::try_join_all(timeout_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let receipt_path = packet.receipt_commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(
                &[b"ibc".to_vec(), receipt_path],
                target_height.revision_height,
            )
            .await?;

        if !value.is_empty() {
            anyhow::bail!("Non-Membership value is empty")
        }
        msg.proof_unreceived = proof.encode_vec();
        msg.proof_height = Some(*target_height);
        anyhow::Ok(())
    }))
    .await?;

    Ok(())
}

/// Generates and injects Ethereum proofs for rec, ack and timeout messages.
/// # Errors
/// Returns an error if a proof cannot be generated for any of the provided messages.
/// # Panics
/// Panics if the provided messages do not contain a valid packet.
#[allow(clippy::too_many_arguments)]
pub async fn inject_ethereum_proofs<P: Provider + Clone>(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
    eth_client: &EthApiClient<P>,
    beacon_api_client: &BeaconApiClient,
    ibc_contract_address: &str,
    ibc_contract_slot: U256,
    proof_slot: u64,
) -> Result<()> {
    let current_beacon_block = beacon_api_client
        .beacon_block(&format!("{proof_slot:?}"))
        .await?;

    let proof_block_number = current_beacon_block
        .message
        .body
        .execution_payload
        .block_number;

    let proof_slot_height = Height {
        revision_number: 0,
        revision_height: proof_slot,
    };

    let account_proof =
        get_account_proof(eth_client, ibc_contract_address, proof_block_number).await?;

    // recv messages
    future::try_join_all(recv_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let commitment_path = packet.commitment_path();
        let storage_proof = get_storage_proof(
            eth_client,
            ibc_contract_address,
            proof_block_number,
            commitment_path,
            ibc_contract_slot,
        )
        .await?;
        if storage_proof.value.is_zero() {
            anyhow::bail!("Membership value is empty")
        }

        let membership_proof = MembershipProof {
            account_proof: account_proof.clone(),
            storage_proof,
        };
        msg.proof_commitment = serde_json::to_vec(&membership_proof)?;
        msg.proof_height = Some(proof_slot_height);
        anyhow::Ok(())
    }))
    .await?;

    // ack messages
    future::try_join_all(ack_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let ack_path = packet.ack_commitment_path();
        let storage_proof = get_storage_proof(
            eth_client,
            ibc_contract_address,
            proof_block_number,
            ack_path,
            ibc_contract_slot,
        )
        .await?;
        if storage_proof.value.is_zero() {
            anyhow::bail!("Membership value is empty")
        }

        let membership_proof = MembershipProof {
            account_proof: account_proof.clone(),
            storage_proof,
        };
        msg.proof_acked = serde_json::to_vec(&membership_proof)?;
        msg.proof_height = Some(proof_slot_height);
        anyhow::Ok(())
    }))
    .await?;

    // timeout messages
    future::try_join_all(timeout_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let receipt_path = packet.receipt_commitment_path();
        let storage_proof = get_storage_proof(
            eth_client,
            ibc_contract_address,
            proof_block_number,
            receipt_path,
            ibc_contract_slot,
        )
        .await?;
        if !storage_proof.value.is_zero() {
            anyhow::bail!("Non-Membership value is empty")
        }

        let membership_proof = MembershipProof {
            account_proof: account_proof.clone(),
            storage_proof,
        };
        msg.proof_unreceived = serde_json::to_vec(&membership_proof)?;
        msg.proof_height = Some(proof_slot_height);
        anyhow::Ok(())
    }))
    .await?;

    Ok(())
}

async fn get_storage_proof<P: Provider + Clone>(
    eth_client: &EthApiClient<P>,
    ibc_contract_address: &str,
    block_number: u64,
    path: Vec<u8>,
    slot: U256,
) -> Result<StorageProof> {
    let storage_key = evm_ics26_commitment_path(&path, slot);
    let storage_key_be_bytes = storage_key.to_be_bytes_vec();
    let storage_key_hex = hex::encode(storage_key_be_bytes);
    let block_hex = format!("0x{block_number:x}");

    let proof = eth_client
        .get_proof(ibc_contract_address, vec![storage_key_hex], block_hex)
        .await?;
    let storage_proof = proof.storage_proof.first().unwrap();

    Ok(StorageProof {
        key: storage_proof.key.as_b256(),
        value: storage_proof.value,
        proof: storage_proof.proof.clone(),
    })
}

async fn get_account_proof<P: Provider + Clone>(
    eth_client: &EthApiClient<P>,
    ibc_contract_address: &str,
    block_number: u64,
) -> Result<AccountProof> {
    let proof = eth_client
        .get_proof(ibc_contract_address, vec![], format!("0x{block_number:x}"))
        .await?;

    Ok(AccountProof {
        proof: proof.account_proof,
        storage_root: proof.storage_hash,
    })
}

/// Injects mock proofs into the provided messages for testing purposes.
pub fn inject_mock_proofs(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
) {
    for msg in recv_msgs.iter_mut() {
        msg.proof_commitment = b"mock".to_vec();
        msg.proof_height = Some(Height::default());
    }

    for msg in ack_msgs.iter_mut() {
        msg.proof_acked = b"mock".to_vec();
        msg.proof_height = Some(Height::default());
    }

    for msg in timeout_msgs.iter_mut() {
        msg.proof_unreceived = b"mock".to_vec();
        msg.proof_height = Some(Height::default());
    }
}
