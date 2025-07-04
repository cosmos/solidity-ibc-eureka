//! Define the events that can be retrieved by the relayer.

use alloy::{
    primitives::{hex, Bytes},
    rpc::types::Log,
    sol_types::{SolEvent, SolEventInterface},
};
use ibc_eureka_solidity_types::ics26::{
    router::{routerEvents, SendPacket, WriteAcknowledgement},
    IICS26RouterMsgs::Packet as SolPacket,
};
use ibc_proto_eureka::ibc::core::channel::v2::{Acknowledgement, Packet};
use prost::Message;
use tendermint::abci::Event as TmEvent;

use super::cosmos_sdk;

/// Events emitted by IBC Eureka implementations that the relayer is interested in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::module_name_repetitions)]
pub struct EurekaEventWithHeight {
    /// The type of the event.
    pub event: EurekaEvent,
    /// The height at which the event was emitted.
    pub height: u64,
}

/// The event type
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::module_name_repetitions)]
pub enum EurekaEvent {
    /// A packet was sent.
    SendPacket(SolPacket),
    /// An acknowledgement was written.
    WriteAcknowledgement(SolPacket, Vec<Bytes>),
}

impl EurekaEventWithHeight {
    /// Get the signature of the events for EVM.
    /// This is used to filter the logs.
    #[must_use]
    pub const fn evm_signatures() -> [&'static str; 2] {
        [SendPacket::SIGNATURE, WriteAcknowledgement::SIGNATURE]
    }
}

impl TryFrom<&Log> for EurekaEventWithHeight {
    type Error = anyhow::Error;

    fn try_from(log: &Log) -> anyhow::Result<Self> {
        let sol_event = routerEvents::decode_log(&log.inner).map_err(|e| {
            anyhow::anyhow!(
                "Failed to decode log into ICS26Router event: {}",
                e.to_string()
            )
        })?;
        let event_type = match sol_event.data {
            routerEvents::SendPacket(event) => Ok(EurekaEvent::SendPacket(event.packet)),
            routerEvents::WriteAcknowledgement(event) => Ok(EurekaEvent::WriteAcknowledgement(
                event.packet,
                event.acknowledgements,
            )),
            routerEvents::AckPacket(_) => Err(anyhow::anyhow!("AckPacket event is not used")),
            routerEvents::TimeoutPacket(_) => {
                Err(anyhow::anyhow!("TimeoutPacket event is not used"))
            }
            _ => Err(anyhow::anyhow!("Unwanted event type: {:?}", sol_event.data)),
        }?;

        Ok(Self {
            event: event_type,
            height: log
                .block_number
                .ok_or_else(|| anyhow::anyhow!("Block number not found in log: {:?}", log))?,
        })
    }
}

impl TryFrom<TmEvent> for EurekaEvent {
    type Error = anyhow::Error;

    fn try_from(event: TmEvent) -> anyhow::Result<Self> {
        match event.kind.as_str() {
            cosmos_sdk::EVENT_TYPE_SEND_PACKET => event
                .attributes
                .into_iter()
                .find_map(|attr| {
                    if attr.key_str().ok()? != cosmos_sdk::ATTRIBUTE_KEY_ENCODED_PACKET_HEX {
                        return None;
                    }
                    let packet: Vec<u8> = hex::decode(attr.value_str().ok()?).ok()?;
                    let packet = Packet::decode(packet.as_slice()).ok()?;
                    Some(Self::SendPacket(packet.into()))
                })
                .ok_or_else(|| anyhow::anyhow!("No packet data found")),
            cosmos_sdk::EVENT_TYPE_WRITE_ACK => {
                let (ack, packet) = event
                    .attributes
                    .into_iter()
                    .filter_map(|attr| match attr.key_str().ok()? {
                        cosmos_sdk::ATTRIBUTE_KEY_ENCODED_ACK_HEX => {
                            let ack_data = hex::decode(attr.value_str().ok()?).ok()?;
                            let ack = Acknowledgement::decode(ack_data.as_slice()).ok()?;
                            Some((Some(ack), None))
                        }
                        cosmos_sdk::ATTRIBUTE_KEY_ENCODED_PACKET_HEX => {
                            let packet_data = hex::decode(attr.value_str().ok()?).ok()?;
                            let packet = Packet::decode(packet_data.as_slice()).ok()?;
                            Some((None, Some(packet)))
                        }
                        _ => None,
                    })
                    .fold((None, None), |(ack_acc, packet_acc), (ack, packet)| {
                        (ack.or(ack_acc), packet.or(packet_acc))
                    });

                Ok(Self::WriteAcknowledgement(
                    packet
                        .ok_or_else(|| anyhow::anyhow!("No packet data found"))?
                        .into(),
                    ack.ok_or_else(|| anyhow::anyhow!("No ack data found"))?
                        .app_acknowledgements
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                ))
            }
            cosmos_sdk::EVENT_TYPE_ACKNOWLEDGE_PACKET
            | cosmos_sdk::EVENT_TYPE_TIMEOUT_PACKET
            | cosmos_sdk::EVENT_TYPE_RECV_PACKET => Err(anyhow::anyhow!("Not implemented")),
            _ => Err(anyhow::anyhow!("Unwanted event type: {}", event.kind)),
        }
    }
}
