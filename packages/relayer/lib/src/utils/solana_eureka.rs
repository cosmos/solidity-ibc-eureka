//! Relayer utilities for `solana-eureka` chains.
use anyhow::Context;
use solana_ibc_types::{
    IbcHeight, MsgAckPacket as SolanaAckPacket, MsgRecvPacket as SolanaMsgRecvPacket,
    MsgTimeoutPacket, Packet, Payload, PayloadMetadata, ProofMetadata,
};

use ibc_proto_eureka::ibc::core::channel::v2::{
    MsgAcknowledgement as IbcMsgAcknowledgement, MsgRecvPacket as IbcMsgRecvPacket,
    Payload as IbcPayload,
};

use crate::events::{SolanaEurekaEvent, SolanaEurekaEventWithHeight};

use tracing;

/// Maximum size for a chunk (matches `CHUNK_DATA_SIZE` in Solana program)
const MAX_CHUNK_SIZE: usize = 700;

/// Threshold for inlining payloads vs chunking (combined size)
const INLINE_THRESHOLD: usize = 300;

/// Result of converting IBC recv packet to Solana format with chunk data
pub struct RecvPacketWithChunks {
    /// The converted Solana recv packet message
    pub msg: SolanaMsgRecvPacket,
    /// Raw payload data for each payload (used for chunking)
    pub payload_chunks: Vec<Vec<u8>>,
    /// Raw proof data (used for chunking)
    pub proof_chunks: Vec<u8>,
}

/// Result of converting IBC ack packet to Solana format with chunk data
pub struct AckPacketWithChunks {
    /// The converted Solana ack packet message
    pub msg: SolanaAckPacket,
    /// Raw payload data for each payload (used for chunking)
    pub payload_chunks: Vec<Vec<u8>>,
    /// Raw proof data (used for chunking)
    pub proof_chunks: Vec<u8>,
}

/// Result of converting timeout events to Solana format with chunk data
pub struct TimeoutPacketWithChunks {
    /// The converted Solana timeout packet message
    pub msg: MsgTimeoutPacket,
    /// Raw payload data for each payload (used for chunking)
    pub payload_chunks: Vec<Vec<u8>>,
    /// Raw proof data (used for chunking)
    pub proof_chunks: Vec<u8>,
}

fn convert_payload(payload: IbcPayload) -> Payload {
    Payload {
        source_port: payload.source_port,
        dest_port: payload.destination_port,
        version: payload.version,
        encoding: payload.encoding,
        value: payload.value,
    }
}

/// Converts an IBC domain `ConsensusState` to Solana IBC `ConsensusState` format.
///
/// # Arguments
/// * `tm_consensus_state` - Tendermint consensus state from IBC domain types
///
/// # Returns
/// * `Ok(ConsensusState)` - Successfully converted Solana IBC consensus state
/// * `Err` - If root or `next_validators_hash` have invalid length
///
/// # Errors
/// - Root is not exactly 32 bytes
/// - Next validators hash is not exactly 32 bytes
pub fn convert_consensus_state(
    tm_consensus_state: &ibc_client_tendermint_types::ConsensusState,
) -> anyhow::Result<solana_ibc_types::ConsensusState> {
    Ok(solana_ibc_types::ConsensusState {
        timestamp: u64::try_from(tm_consensus_state.timestamp.unix_timestamp_nanos())
            .context("incorrect consensus timestamp")?,
        root: tm_consensus_state
            .root
            .as_bytes()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid root length"))?,
        next_validators_hash: tm_consensus_state
            .next_validators_hash
            .as_bytes()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid next_validators_hash length"))?,
    })
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
pub fn convert_client_state_to_sol(
    ibc_client: ibc_proto_eureka::ibc::lightclients::tendermint::v1::ClientState,
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

/// Converts a Solana IBC `ClientState` to IBC protobuf `ClientState` format.
///
/// # Arguments
/// * `solana_client` - Solana IBC client state format
///
/// # Errors
/// * Returns an error if any of the following duration fields exceed `i64::MAX`:
///   - `trusting_period`
///   - `unbonding_period`
///   - `max_clock_drift`
///
/// # Returns
/// * `Ok(ClientState)` - Successfully converted IBC protobuf client state
///
/// # Note
/// - Durations are converted from seconds (u64) to protobuf Duration format
/// - Proof specs are set to default ICS23 specs (IAVL and Tendermint)
/// - Upgrade path is left empty as it's not used in Solana
pub fn convert_client_state_to_ibc(
    solana_client: solana_ibc_types::ClientState,
) -> anyhow::Result<ibc_proto_eureka::ibc::lightclients::tendermint::v1::ClientState> {
    use ibc_proto_eureka::google::protobuf::Duration;
    use ibc_proto_eureka::ibc::core::client::v1::Height;
    use ibc_proto_eureka::ibc::lightclients::tendermint::v1::Fraction;

    let trust_level = Some(Fraction {
        numerator: solana_client.trust_level_numerator,
        denominator: solana_client.trust_level_denominator,
    });

    let trusting_period = Some(Duration {
        seconds: i64::try_from(solana_client.trusting_period)
            .map_err(|_| anyhow::anyhow!("Trusting period exceeds i64::MAX"))?,
        nanos: 0,
    });

    let unbonding_period = Some(Duration {
        seconds: i64::try_from(solana_client.unbonding_period)
            .map_err(|_| anyhow::anyhow!("Unbonding period exceeds i64::MAX"))?,
        nanos: 0,
    });

    let max_clock_drift = Some(Duration {
        seconds: i64::try_from(solana_client.max_clock_drift)
            .map_err(|_| anyhow::anyhow!("Max clock drift exceeds i64::MAX"))?,
        nanos: 0,
    });

    let frozen_height = if solana_client.frozen_height.revision_number == 0
        && solana_client.frozen_height.revision_height == 0
    {
        None
    } else {
        Some(Height {
            revision_number: solana_client.frozen_height.revision_number,
            revision_height: solana_client.frozen_height.revision_height,
        })
    };

    let latest_height = Some(Height {
        revision_number: solana_client.latest_height.revision_number,
        revision_height: solana_client.latest_height.revision_height,
    });

    let proof_specs = vec![ics23::iavl_spec(), ics23::tendermint_spec()];

    #[allow(deprecated)]
    Ok(
        ibc_proto_eureka::ibc::lightclients::tendermint::v1::ClientState {
            chain_id: solana_client.chain_id,
            trust_level,
            trusting_period,
            unbonding_period,
            max_clock_drift,
            frozen_height,
            latest_height,
            proof_specs,
            upgrade_path: vec![],                  // Not used in Solana
            allow_update_after_expiry: true,       // Deprecated but required field
            allow_update_after_misbehaviour: true, // Deprecated but required field
        },
    )
}

/// Converts a list of [`SolanaEurekaEvent`]s to a list of timeout packets with chunk data.
///
/// # Arguments
/// - `target_events` - The list of target events.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `dst_packet_seqs` - The list of dest packet sequences to filter. If empty, no filtering.
/// - `target_height`: The target height for the proofs.
/// - `now` - The current time.
///
/// # Panics
/// too big payload/proof
#[must_use]
pub fn target_events_to_timeout_msgs(
    target_events: Vec<SolanaEurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    dst_packet_seqs: &[u64],
    target_height: u64,
    now: u64,
) -> Vec<TimeoutPacketWithChunks> {
    target_events
        .into_iter()
        .filter_map(|e| match e.event {
            SolanaEurekaEvent::SendPacket(event) => (now
                >= u64::try_from(event.timeout_timestamp).unwrap_or_default()
                && event.packet.source_client == dst_client_id
                && event.packet.dest_client == src_client_id
                && (dst_packet_seqs.is_empty()
                    || dst_packet_seqs.contains(&event.packet.sequence)))
            .then_some({
                // Extract raw payload data for chunking
                let payload_chunks: Vec<Vec<u8>> = event
                    .packet
                    .payloads
                    .iter()
                    .map(|p| p.value.clone())
                    .collect();

                let sequence = event.packet.sequence;

                // Build metadata using helper function
                let payloads_metadata = build_metadata_from_solana_payloads(
                    &event.packet.payloads,
                    sequence,
                )
                .expect("Failed to build payload metadata");

                tracing::info!(
                    "timeout_packet seq={}: metadata.len()={}, proof will be filled later",
                    sequence,
                    payloads_metadata.len()
                );

                TimeoutPacketWithChunks {
                    msg: MsgTimeoutPacket {
                        packet: event.packet,
                        payloads: payloads_metadata,
                        proof: ProofMetadata {
                            height: target_height,
                            total_chunks: 0,
                        },
                    },
                    payload_chunks,
                    proof_chunks: vec![], // Will be filled later with actual proof data
                }
            }),
            SolanaEurekaEvent::WriteAcknowledgement(..) => None,
        })
        .collect()
}

/// Injects mock proofs into the provided messages for testing purposes.
pub fn inject_mock_proofs(timeout_msgs: &mut [TimeoutPacketWithChunks]) {
    for timeout_with_chunks in timeout_msgs.iter_mut() {
        // Update proof metadata with mock values
        timeout_with_chunks.msg.proof.total_chunks = 0; // No chunking for mock proof
        timeout_with_chunks.msg.proof.height = 0; // Default height for mock
        timeout_with_chunks.proof_chunks = b"mock".to_vec(); // Mock proof data
    }
}

/// Build inline mode metadata from Solana payloads
fn build_inline_metadata_from_solana_payloads(
    payloads: &[Payload],
) -> Vec<PayloadMetadata> {
    payloads
        .iter()
        .map(|p| PayloadMetadata {
            source_port: p.source_port.clone(),
            dest_port: p.dest_port.clone(),
            version: p.version.clone(),
            encoding: p.encoding.clone(),
            total_chunks: 0, // 0 indicates inline mode
        })
        .collect()
}

/// Build chunked mode metadata from Solana payloads
fn build_chunked_metadata_from_solana_payloads(
    payloads: &[Payload],
    sequence: u64,
) -> anyhow::Result<Vec<PayloadMetadata>> {
    payloads
        .iter()
        .map(|p| {
            let total_chunks = u8::try_from(p.value.len().div_ceil(MAX_CHUNK_SIZE).max(1))
                .context("payload too big to fit in u8")?;
            tracing::info!(
                "packet seq={}: payload size={}, chunks={}",
                sequence,
                p.value.len(),
                total_chunks
            );
            Ok::<_, anyhow::Error>(PayloadMetadata {
                source_port: p.source_port.clone(),
                dest_port: p.dest_port.clone(),
                version: p.version.clone(),
                encoding: p.encoding.clone(),
                total_chunks,
            })
        })
        .collect::<Result<Vec<_>, _>>()
}

/// Build payload metadata from Solana payloads, handling inline vs chunked mode
fn build_metadata_from_solana_payloads(
    payloads: &[Payload],
    sequence: u64,
) -> anyhow::Result<Vec<PayloadMetadata>> {
    let total_payload_size: usize = payloads.iter().map(|p| p.value.len()).sum();
    let is_inline = total_payload_size < INLINE_THRESHOLD;

    let mode_str = if is_inline { "INLINE" } else { "CHUNKED" };
    tracing::info!(
        "packet seq={}: {} mode (total_size={}, threshold={}, num_payloads={})",
        sequence,
        mode_str,
        total_payload_size,
        INLINE_THRESHOLD,
        payloads.len()
    );

    if is_inline {
        Ok(build_inline_metadata_from_solana_payloads(payloads))
    } else {
        build_chunked_metadata_from_solana_payloads(payloads, sequence)
    }
}

/// Build inline mode packet with all payloads included
fn build_inline_packet(
    sequence: u64,
    source_client: String,
    dest_client: String,
    timeout_timestamp: u64,
    payloads: &[IbcPayload],
) -> (Packet, Vec<PayloadMetadata>) {
    let solana_payloads = payloads
        .iter()
        .map(|p| convert_payload(p.clone()))
        .collect();

    let payloads_metadata: Vec<PayloadMetadata> = payloads
        .iter()
        .map(|p| PayloadMetadata {
            source_port: p.source_port.clone(),
            dest_port: p.destination_port.clone(),
            version: p.version.clone(),
            encoding: p.encoding.clone(),
            total_chunks: 0, // 0 indicates inline mode
        })
        .collect();

    let packet = Packet {
        sequence,
        source_client,
        dest_client,
        timeout_timestamp: i64::try_from(timeout_timestamp).unwrap_or_default(),
        payloads: solana_payloads,
    };

    (packet, payloads_metadata)
}

fn build_chunked_packet(
    sequence: u64,
    source_client: String,
    dest_client: String,
    timeout_timestamp: u64,
    payloads: Vec<IbcPayload>,
) -> anyhow::Result<(Packet, Vec<PayloadMetadata>)> {
    let payloads_metadata: Vec<PayloadMetadata> = payloads
        .into_iter()
        .map(|p| {
            let total_chunks = u8::try_from(p.value.len().div_ceil(MAX_CHUNK_SIZE).max(1))
                .context("payload too big to fit in u8")?;
            tracing::info!(
                "packet seq={}: payload size={}, chunks={}",
                sequence,
                p.value.len(),
                total_chunks
            );
            Ok::<_, anyhow::Error>(PayloadMetadata {
                source_port: p.source_port,
                dest_port: p.destination_port,
                version: p.version,
                encoding: p.encoding,
                total_chunks,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let packet = Packet {
        sequence,
        source_client,
        dest_client,
        timeout_timestamp: i64::try_from(timeout_timestamp).unwrap_or_default(),
        payloads: vec![], // Empty for chunked mode
    };

    Ok((packet, payloads_metadata))
}

/// Builds a Solana packet with payloads and metadata, handling inline vs chunked mode.
fn build_packet_with_payloads(
    sequence: u64,
    source_client: String,
    dest_client: String,
    timeout_timestamp: u64,
    payloads: Vec<IbcPayload>,
) -> anyhow::Result<(Packet, Vec<PayloadMetadata>, Vec<Vec<u8>>)> {
    let payload_chunks: Vec<Vec<u8>> = payloads.iter().map(|p| p.value.clone()).collect();

    // Calculate total payload size to determine inline vs chunked
    let total_payload_size: usize = payloads.iter().map(|p| p.value.len()).sum();
    let is_inline = total_payload_size < INLINE_THRESHOLD;

    let mode_str = if is_inline { "INLINE" } else { "CHUNKED" };
    tracing::info!(
        "packet seq={}: {} mode (total_size={}, threshold={}, num_payloads={})",
        sequence,
        mode_str,
        total_payload_size,
        INLINE_THRESHOLD,
        payloads.len()
    );

    let (packet, payloads_metadata) = if is_inline {
        build_inline_packet(
            sequence,
            source_client,
            dest_client,
            timeout_timestamp,
            &payloads,
        )
    } else {
        build_chunked_packet(
            sequence,
            source_client,
            dest_client,
            timeout_timestamp,
            payloads,
        )?
    };

    tracing::info!(
        "packet seq={}: built with {} payloads in packet, {} metadata entries",
        sequence,
        packet.payloads.len(),
        payloads_metadata.len()
    );

    Ok((packet, payloads_metadata, payload_chunks))
}

/// Convert IBC cosmos message types to Solana `MsgRecvPacket` with chunk data
///
/// # Arguments
/// * `value` - IBC protobuf `MsgRecvPacket` containing packet data and proofs
///
/// # Returns
/// * `Ok(RecvPacketWithChunks)` - Successfully converted message with chunk data
/// * `Err` - If required fields are missing or invalid
///
/// # Errors
/// - Missing packet in the input message
/// - Missing proof commitment
/// - Missing proof height
/// - Invalid timeout timestamp (exceeds `i64::MAX`)
pub fn ibc_to_solana_recv_packet(value: IbcMsgRecvPacket) -> anyhow::Result<RecvPacketWithChunks> {
    let ibc_packet = value
        .packet
        .ok_or_else(|| anyhow::anyhow!("Missing packet in MsgRecvPacket"))?;

    let proof_height = value
        .proof_height
        .ok_or_else(|| anyhow::anyhow!("Missing proof height"))?;

    if ibc_packet.payloads.is_empty() {
        return Err(anyhow::anyhow!("Packet payloads cannot be empty"));
    }

    // Build packet and metadata using helper function
    let (packet, payloads_metadata, payload_chunks) = build_packet_with_payloads(
        ibc_packet.sequence,
        ibc_packet.source_client,
        ibc_packet.destination_client,
        ibc_packet.timeout_timestamp,
        ibc_packet.payloads,
    )?;

    // Create proof metadata
    let proof_chunks = value.proof_commitment.clone();
    let proof_total_chunks =
        u8::try_from(value.proof_commitment.len().div_ceil(MAX_CHUNK_SIZE).max(1))
            .context("proof too big to fit in u8")?;

    tracing::info!(
        "recv_packet seq={}: proof_size={}, proof_chunks={}",
        packet.sequence,
        value.proof_commitment.len(),
        proof_total_chunks
    );

    let proof_metadata = ProofMetadata {
        height: proof_height.revision_height,
        total_chunks: proof_total_chunks,
    };

    Ok(RecvPacketWithChunks {
        msg: SolanaMsgRecvPacket {
            packet,
            payloads: payloads_metadata,
            proof: proof_metadata,
        },
        payload_chunks,
        proof_chunks,
    })
}

/// Convert IBC cosmos message types to Solana `MsgAckPacket` with chunk data
///
/// # Arguments
/// * `value` - IBC protobuf `MsgAcknowledgement` containing packet data and proofs
///
/// # Returns
/// * `Ok(AckPacketWithChunks)` - Successfully converted message with chunk data
/// * `Err` - If required fields are missing or invalid
///
/// # Errors
/// - Missing packet in the input message
/// - Missing acknowledgements
/// - Missing proof acked
/// - Missing proof height
/// - Invalid timeout timestamp (exceeds `i64::MAX`)
#[allow(clippy::cognitive_complexity)]
pub fn ibc_to_solana_ack_packet(
    value: IbcMsgAcknowledgement,
) -> anyhow::Result<AckPacketWithChunks> {
    let ibc_packet = value
        .packet
        .ok_or_else(|| anyhow::anyhow!("Missing packet in MsgAcknowledgement"))?;

    let acknowledgement = value
        .acknowledgement
        .ok_or_else(|| anyhow::anyhow!("Missing acknowledgements"))?;

    if acknowledgement.app_acknowledgements.is_empty() {
        return Err(anyhow::anyhow!("Acknowledgements cannot be empty"));
    }
    let acknowledgement_data = acknowledgement.app_acknowledgements[0].clone();

    let proof_height = value
        .proof_height
        .ok_or_else(|| anyhow::anyhow!("Missing proof height"))?;

    if ibc_packet.payloads.is_empty() {
        return Err(anyhow::anyhow!("Packet payloads cannot be empty"));
    }

    let (packet, payloads_metadata, payload_chunks) = build_packet_with_payloads(
        ibc_packet.sequence,
        ibc_packet.source_client,
        ibc_packet.destination_client,
        ibc_packet.timeout_timestamp,
        ibc_packet.payloads,
    )?;

    let proof_chunks = value.proof_acked.clone();
    let proof_total_chunks = u8::try_from(value.proof_acked.len().div_ceil(MAX_CHUNK_SIZE).max(1))
        .context("proof too big")?;

    tracing::info!("=== CONVERTING TO SOLANA FORMAT ===");
    tracing::info!("  Packet sequence: {}", packet.sequence);
    tracing::info!("  IBC proof_height from message: {:?}", proof_height);
    tracing::info!(
        "  IBC proof_height.revision_height: {}",
        proof_height.revision_height
    );
    tracing::info!(
        "  Setting Solana proof.height = {}",
        proof_height.revision_height
    );
    tracing::info!("  Proof size: {} bytes", value.proof_acked.len());
    tracing::info!("  Proof chunks: {}", proof_total_chunks);
    tracing::info!("  Ack size: {} bytes", acknowledgement_data.len());

    let proof_metadata = ProofMetadata {
        height: proof_height.revision_height,
        total_chunks: proof_total_chunks,
    };

    tracing::info!("  Final ProofMetadata.height = {}", proof_metadata.height);

    Ok(AckPacketWithChunks {
        msg: SolanaAckPacket {
            packet,
            payloads: payloads_metadata,
            acknowledgement: acknowledgement_data,
            proof: proof_metadata,
        },
        payload_chunks,
        proof_chunks,
    })
}
