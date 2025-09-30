//! Utility functions for Solana to Cosmos IBC message conversion.

use ibc_proto_eureka::ibc::core::channel::v2::{
    Acknowledgement, MsgAcknowledgement, MsgRecvPacket, MsgTimeout, Packet, Payload,
};

use crate::events::solana::{SolanaEurekaEvent, SolanaEurekaEventWithHeight};

/// Convert Solana events to receive packet and acknowledgement messages for Cosmos.
///
/// # Arguments
/// - `src_events` - The list of source events from Solana.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `src_packet_seqs` - The list of source packet sequences to filter. If empty, no filtering.
/// - `dst_packet_seqs` - The list of dest packet sequences to filter. If empty, no filtering.
/// - `signer_address` - The signer address.
/// - `now` - The current time.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn src_events_to_recv_and_ack_msgs(
    src_events: Vec<SolanaEurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    src_packet_seqs: &[u64],
    dst_packet_seqs: &[u64],
    signer_address: &str,
    now: u64,
) -> (Vec<MsgRecvPacket>, Vec<MsgAcknowledgement>) {
    let mut recv_msgs = Vec::new();
    let mut ack_msgs = Vec::new();

    for ev_with_height in src_events {
        match ev_with_height.event {
            SolanaEurekaEvent::SendPacket(event) => {
                if event.timeout_timestamp > now as i64
                    && event.packet.source_client == src_client_id
                    && event.packet.dest_client == dst_client_id
                    && (src_packet_seqs.is_empty()
                        || src_packet_seqs.contains(&event.packet.sequence))
                {
                    let ibc_packet = Packet {
                        sequence: event.sequence,
                        source_client: event.packet.source_client.clone(),
                        destination_client: event.packet.dest_client.clone(),
                        timeout_timestamp: u64::try_from(event.timeout_timestamp).unwrap_or(O),
                        payloads: event.packet
                            .payloads
                            .iter()
                            .map(|p| Payload {
                                source_port: "transfer".to_string(),
                                destination_port: "transfer".to_string(),
                                version: "ics20-1".to_string(),
                                encoding: "application/json".to_string(),
                                value: p.value.clone(),
                            })
                            .collect(),
                    };
                }
            }
            SolanaEurekaEvent::WriteAcknowledgement(write_acknowledgement_event) => {
                ev_with_height.packet.source_client == dst_client_id
                    && ev_with_height.packet.dest_client == src_client_id
                    && (dst_packet_seqs.is_empty()
                        || dst_packet_seqs.contains(&ev_with_height.packet.sequence)) {

                }
            },
        }
    }

    let mut recv_msgs = Vec::new();
    let mut ack_msgs = Vec::new();

    for event_with_height in src_events {
        match event_with_height.event {
            SolanaEurekaEvent::SendPacket(send_event) => {
                let packet = &send_event.packet;

                // Convert to IBC v2 packet format
                let ibc_packet = Packet {
                    sequence: send_event.sequence,
                    source_client: packet.source_client.clone(),
                    destination_client: packet.dest_client.clone(),
                    timeout_timestamp,
                    payloads: packet
                        .payloads
                        .iter()
                        .map(|p| Payload {
                            source_port: "transfer".to_string(),
                            destination_port: "transfer".to_string(),
                            version: "ics20-1".to_string(),
                            encoding: "application/json".to_string(),
                            value: p.value.clone(),
                        })
                        .collect(),
                };

                recv_msgs.push(MsgRecvPacket {
                    packet: Some(ibc_packet),
                    proof_height: None,
                    proof_commitment: vec![],
                    signer: signer_address.to_string(),
                });
            }
            SolanaEurekaEvent::WriteAcknowledgement(write_ack) => {
                let packet = &write_ack.packet;

                // For acknowledgements, the packet direction is reversed
                if packet.source_client != dst_client_id || packet.dest_client != src_client_id {
                    continue; // Skip acks for different clients
                }

                if !dst_packet_seqs.is_empty() && !dst_packet_seqs.contains(&write_ack.sequence) {
                    continue; // Skip if sequence filtering is enabled and doesn't match
                }

                let ibc_packet = Packet {
                    sequence: write_ack.sequence,
                    source_client: packet.source_client.clone(),
                    destination_client: packet.dest_client.clone(),
                    timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                    payloads: packet
                        .payloads
                        .iter()
                        .map(|p| Payload {
                            source_port: "transfer".to_string(),
                            destination_port: "transfer".to_string(),
                            version: "ics20-1".to_string(),
                            encoding: "application/json".to_string(),
                            value: p.value.clone(),
                        })
                        .collect(),
                };

                ack_msgs.push(MsgAcknowledgement {
                    packet: Some(ibc_packet),
                    acknowledgement: Some(Acknowledgement {
                        app_acknowledgements: vec![write_ack.acknowledgements.clone()],
                    }),
                    proof_height: None,
                    proof_acked: vec![],
                    signer: signer_address.to_string(),
                });
            }
            _ => {
                // Skip non-packet events
            }
        }
    }

    (recv_msgs, ack_msgs)
}

/// Convert destination events (from Cosmos) to timeout messages.
///
/// # Arguments
/// - `dest_events` - The list of destination events.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `packet_seqs` - The list of packet sequences to filter. If empty, no filtering.
/// - `signer_address` - The signer address.
/// - `now` - The current time.
#[must_use]
pub fn target_events_to_timeout_msgs(
    dest_events: Vec<SolanaEurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    packet_seqs: &[u64],
    signer_address: &str,
    _now: u64,
) -> Vec<MsgTimeout> {
    let mut timeout_msgs = Vec::new();

    for event_with_height in dest_events {
        if let SolanaEurekaEvent::TimeoutPacket(timeout_event) = event_with_height.event {
            let packet = &timeout_event.packet;

            // Check if packet is valid for timeout processing
            if packet.source_client != src_client_id || packet.dest_client != dst_client_id {
                continue; // Skip packets for different clients
            }

            if !packet_seqs.is_empty() && !packet_seqs.contains(&timeout_event.sequence) {
                continue; // Skip if sequence filtering is enabled and doesn't match
            }

            // Convert to IBC v2 packet format
            let ibc_packet = Packet {
                sequence: timeout_event.sequence,
                source_client: packet.source_client.clone(),
                destination_client: packet.dest_client.clone(),
                timeout_timestamp: u64::try_from(packet.timeout_timestamp).unwrap_or(0),
                payloads: packet
                    .payloads
                    .iter()
                    .map(|p| Payload {
                        source_port: "transfer".to_string(),
                        destination_port: "transfer".to_string(),
                        version: "ics20-1".to_string(),
                        encoding: "application/json".to_string(),
                        value: p.value.clone(),
                    })
                    .collect(),
            };

            timeout_msgs.push(MsgTimeout {
                packet: Some(ibc_packet),
                proof_height: None,
                proof_unreceived: vec![],
                signer: signer_address.to_string(),
            });
        }
    }

    timeout_msgs
}

/// Inject mock proofs for testing.
///
/// # Arguments
/// - `recv_msgs` - The list of receive packet messages.
/// - `ack_msgs` - The list of acknowledgement messages.
/// - `timeout_msgs` - The list of timeout messages.
pub fn inject_mock_proofs(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
) {
    for msg in recv_msgs {
        msg.proof_commitment = b"mock_proof".to_vec();
    }

    for msg in ack_msgs {
        msg.proof_acked = b"mock_proof".to_vec();
    }

    for msg in timeout_msgs {
        msg.proof_unreceived = b"mock_proof".to_vec();
    }
}
