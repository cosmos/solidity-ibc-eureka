//! Relayer utilities for `solana-eureka` chains.
use solana_ibc_types::{
    IbcHeight, MsgAckPacket as SolanaAckPacket, MsgRecvPacket as SolanaMsgRecvPacket,
    MsgTimeoutPacket, Packet, Payload,
};

use ibc_proto_eureka::ibc::core::channel::v2::{
    MsgAcknowledgement as IbcMsgAcknowledgement, MsgRecvPacket as IbcMsgRecvPacket,
    Payload as IbcPayload,
};

use crate::events::{SolanaEurekaEvent, SolanaEurekaEventWithHeight};

fn convert_payload(payload: IbcPayload) -> Payload {
    Payload {
        source_port: payload.source_port,
        dest_port: payload.destination_port,
        version: payload.version,
        encoding: payload.encoding,
        value: payload.value,
    }
}

/// Converts an IBC protobuf `ClientState` to Solana IBC `ClientState` format.
///
/// # Arguments
/// * `ibc_client` - Tendermint client state from IBC protobuf format
///
/// # Returns
/// * `Ok(ClientState)` - Successfully converted Solana IBC client state
/// * `Err` - If required fields (trust level, periods, latest height) are missing
///
/// # Errors
/// - Missing trust level in the input client state
/// - Missing trusting period in the input client state
/// - Missing unbonding period in the input client state
/// - Missing max clock drift in the input client state
/// - Missing latest height in the input client state
/// - Duration seconds exceed `u64::MAX` (silently defaults to 0)
///
/// # Note
/// - Durations are converted to seconds (u64)
/// - Frozen height defaults to 0/0 if not set
/// - Proof specs are not included in the conversion as Solana Tendemint client hardcodes them
pub fn convert_client_state(
    ibc_client: ibc_proto::ibc::lightclients::tendermint::v1::ClientState,
) -> anyhow::Result<solana_ibc_types::ClientState> {
    let trust_level = ibc_client
        .trust_level
        .ok_or_else(|| anyhow::anyhow!("Missing trust level"))?;

    let trusting_period = ibc_client
        .trusting_period
        .ok_or_else(|| anyhow::anyhow!("Missing trusting period"))?;

    let unbonding_period = ibc_client
        .unbonding_period
        .ok_or_else(|| anyhow::anyhow!("Missing unbonding period"))?;

    let max_clock_drift = ibc_client
        .max_clock_drift
        .ok_or_else(|| anyhow::anyhow!("Missing max clock drift"))?;

    let frozen_height = ibc_client.frozen_height.map_or(
        IbcHeight {
            revision_number: 0,
            revision_height: 0,
        },
        |h| IbcHeight {
            revision_number: h.revision_number,
            revision_height: h.revision_height,
        },
    );

    let latest_height = ibc_client
        .latest_height
        .ok_or_else(|| anyhow::anyhow!("Missing latest height"))?;

    Ok(solana_ibc_types::ClientState {
        chain_id: ibc_client.chain_id,
        trust_level_numerator: trust_level.numerator,
        trust_level_denominator: trust_level.denominator,
        trusting_period: u64::try_from(trusting_period.seconds).unwrap_or_default(),
        unbonding_period: u64::try_from(unbonding_period.seconds).unwrap_or_default(),
        max_clock_drift: u64::try_from(max_clock_drift.seconds).unwrap_or_default(),
        frozen_height,
        latest_height: IbcHeight {
            revision_number: latest_height.revision_number,
            revision_height: latest_height.revision_height,
        },
    })
}

/// Converts a list of [`SolanaEurekaEvent`]s to a list of [`MsgTimeout`]s.
///
/// # Arguments
/// - `target_events` - The list of target events.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `dst_packet_seqs` - The list of dest packet sequences to filter. If empty, no filtering.
/// - `target_height`: The target height for the proofs.
/// - `now` - The current time.
#[must_use]
pub fn target_events_to_timeout_msgs(
    target_events: Vec<SolanaEurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    dst_packet_seqs: &[u64],
    target_height: u64,
    now: u64,
) -> Vec<MsgTimeoutPacket> {
    target_events
        .into_iter()
        .filter_map(|e| match e.event {
            SolanaEurekaEvent::SendPacket(event) => (now
                >= u64::try_from(event.timeout_timestamp).unwrap_or_default()
                && event.packet.source_client == dst_client_id
                && event.packet.dest_client == src_client_id
                && (dst_packet_seqs.is_empty()
                    || dst_packet_seqs.contains(&event.packet.sequence)))
            .then_some(MsgTimeoutPacket {
                packet: event.packet,
                proof_height: target_height,
                proof_timeout: vec![],
            }),
            SolanaEurekaEvent::WriteAcknowledgement(..) => None,
        })
        .collect()
}

/// Convert IBC cosmos message types to Solana `MsgRecvPacket`
///
/// # Arguments
/// * `value` - IBC protobuf `MsgRecvPacket` containing packet data and proofs
///
/// # Returns
/// * `Ok(SolanaMsgRecvPacket)` - Successfully converted message
/// * `Err` - If required fields are missing or invalid
///
/// # Errors
/// - Missing packet in the input message
/// - Missing proof commitment
/// - Missing proof height
/// - Invalid timeout timestamp (exceeds `i64::MAX`)
pub fn ibc_to_solana_recv_packet(value: IbcMsgRecvPacket) -> anyhow::Result<SolanaMsgRecvPacket> {
    let ibc_packet = value
        .packet
        .ok_or_else(|| anyhow::anyhow!("Missing packet in MsgRecvPacket"))?;

    let proof_commitment = value.proof_commitment;

    let proof_height = value
        .proof_height
        .ok_or_else(|| anyhow::anyhow!("Missing proof height"))?;

    if ibc_packet.payloads.is_empty() {
        return Err(anyhow::anyhow!("Packet payloads cannot be empty"));
    }

    let payloads = ibc_packet
        .payloads
        .into_iter()
        .map(convert_payload)
        .collect();

    let packet = Packet {
        sequence: ibc_packet.sequence,
        source_client: ibc_packet.source_client,
        dest_client: ibc_packet.destination_client,
        timeout_timestamp: i64::try_from(ibc_packet.timeout_timestamp).unwrap_or_default(),
        payloads,
    };

    Ok(SolanaMsgRecvPacket {
        packet,
        proof_commitment,
        proof_height: proof_height.revision_height,
    })
}

/// Convert IBC cosmos message types to Solana `MsgAckPacket`
///
/// # Arguments
/// * `value` - IBC protobuf `MsgAcknowledgement` containing packet data and proofs
///
/// # Returns
/// * `Ok(SolanaAckPacket)` - Successfully converted message
/// * `Err` - If required fields are missing or invalid
///
/// # Errors
/// - Missing packet in the input message
/// - Missing acknowledgements
/// - Missing proof acked
/// - Missing proof height
/// - Invalid timeout timestamp (exceeds `i64::MAX`)
pub fn ibc_to_solana_ack_packet(value: IbcMsgAcknowledgement) -> anyhow::Result<SolanaAckPacket> {
    let ibc_packet = value
        .packet
        .ok_or_else(|| anyhow::anyhow!("Missing packet in MsgAcknowledgement"))?;

    let acknowledgement = value
        .acknowledgement
        .ok_or_else(|| anyhow::anyhow!("Missing acknowledgements"))?;

    if acknowledgement.app_acknowledgements.is_empty() {
        return Err(anyhow::anyhow!("Acknowledgements cannot be empty"));
    }
    let acknowledgement = acknowledgement.app_acknowledgements[0].clone();

    let proof_acked = value.proof_acked;

    let proof_height = value
        .proof_height
        .ok_or_else(|| anyhow::anyhow!("Missing proof height"))?;

    if ibc_packet.payloads.is_empty() {
        return Err(anyhow::anyhow!("Packet payloads cannot be empty"));
    }

    let payloads = ibc_packet
        .payloads
        .into_iter()
        .map(convert_payload)
        .collect();

    let packet = Packet {
        sequence: ibc_packet.sequence,
        source_client: ibc_packet.source_client,
        dest_client: ibc_packet.destination_client,
        timeout_timestamp: i64::try_from(ibc_packet.timeout_timestamp).unwrap_or_default(),
        payloads,
    };

    Ok(SolanaAckPacket {
        packet,
        acknowledgement,
        proof_acked,
        proof_height: proof_height.revision_height,
    })
}
