//! Relayer utilities for `solidity-ibc-eureka` chains.

use anyhow::Result;
use ibc_eureka_solidity_types::ics26::{
    router::{ackPacketCall, recvPacketCall, routerCalls},
    IICS02ClientMsgs::Height,
    IICS26RouterMsgs::{MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket},
};

use crate::events::EurekaEvent;

/// Converts a list of [`EurekaEvent`]s to a list of [`routerCalls::timeoutPacket`]s with empty
/// proofs.
/// # Errors
/// Errors if the current time cannot be fetched.
pub fn target_events_to_timeout_msgs(
    target_events: Vec<EurekaEvent>,
    target_channel_id: &str,
    target_height: &Height,
) -> Result<Vec<routerCalls>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(target_events
        .into_iter()
        .filter_map(|e| match e {
            EurekaEvent::SendPacket(se) => {
                if now >= se.packet.timeoutTimestamp && se.packet.sourceChannel == target_channel_id
                {
                    Some(routerCalls::timeoutPacket(
                        ibc_eureka_solidity_types::ics26::router::timeoutPacketCall {
                            msg_: MsgTimeoutPacket {
                                packet: se.packet,
                                proofHeight: target_height.clone(),
                                proofTimeout: b"".into(),
                            },
                        },
                    ))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect())
}

/// Converts a list of [`EurekaEvent`]s to a list of [`routerCalls::recvPacket`]s and
/// [`routerCalls::ackPacket`]s with empty proofs.
/// # Errors
/// Errors if the current time cannot be fetched.
pub fn src_events_to_recv_and_ack_msgs(
    src_events: Vec<EurekaEvent>,
    target_channel_id: &str,
    target_height: &Height,
) -> Result<Vec<routerCalls>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(src_events
        .into_iter()
        .filter_map(|e| match e {
            EurekaEvent::SendPacket(se) => {
                if se.packet.timeoutTimestamp > now && se.packet.destChannel == target_channel_id {
                    Some(routerCalls::recvPacket(recvPacketCall {
                        msg_: MsgRecvPacket {
                            packet: se.packet,
                            proofHeight: target_height.clone(),
                            proofCommitment: b"".into(),
                        },
                    }))
                } else {
                    None
                }
            }
            EurekaEvent::WriteAcknowledgement(we) => {
                if we.packet.sourceChannel == target_channel_id {
                    Some(routerCalls::ackPacket(ackPacketCall {
                        msg_: MsgAckPacket {
                            packet: we.packet,
                            acknowledgement: we.acknowledgements[0].clone(), // TODO: handle multiple acks
                            proofHeight: target_height.clone(),
                            proofAcked: b"".into(),
                        },
                    }))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect())
}
