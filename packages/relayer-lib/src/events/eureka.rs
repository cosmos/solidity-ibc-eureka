//! Define the events that can be retrieved by the relayer.

use alloy::{hex, sol_types::SolEvent};
use ibc_eureka_solidity_types::ics26::router::{
    routerEvents, AckPacket, RecvPacket, SendPacket, TimeoutPacket, WriteAcknowledgement,
};
use prost::Message;
use tendermint::abci::Event as TmEvent;

use super::cosmos_sdk;

/// Events emitted by IBC Eureka implementations that the relayer is interested in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(clippy::module_name_repetitions)]
pub enum EurekaEvent {
    /// A packet was sent.
    SendPacket(SendPacket),
    /// A packet was received.
    RecvPacket(RecvPacket),
    /// A packet was acknowledged.
    AckPacket(AckPacket),
    /// A packet timed out.
    TimeoutPacket(TimeoutPacket),
    /// An acknowledgement was written.
    WriteAcknowledgement(WriteAcknowledgement),
}

impl EurekaEvent {
    /// Get the signature of the events for EVM.
    /// This is used to filter the logs.
    #[must_use]
    pub const fn evm_signatures() -> [&'static str; 5] {
        [
            SendPacket::SIGNATURE,
            RecvPacket::SIGNATURE,
            AckPacket::SIGNATURE,
            TimeoutPacket::SIGNATURE,
            WriteAcknowledgement::SIGNATURE,
        ]
    }
}

impl TryFrom<routerEvents> for EurekaEvent {
    type Error = anyhow::Error;

    fn try_from(event: routerEvents) -> anyhow::Result<Self> {
        match event {
            routerEvents::SendPacket(event) => Ok(Self::SendPacket(event)),
            routerEvents::RecvPacket(event) => Ok(Self::RecvPacket(event)),
            routerEvents::AckPacket(event) => Ok(Self::AckPacket(event)),
            routerEvents::TimeoutPacket(event) => Ok(Self::TimeoutPacket(event)),
            routerEvents::WriteAcknowledgement(event) => Ok(Self::WriteAcknowledgement(event)),
            routerEvents::Noop(_) => Err(anyhow::anyhow!("Noop event")),
            routerEvents::IBCAppAdded(_) => Err(anyhow::anyhow!("IBCAppAdded event")),
            routerEvents::OwnershipTransferred(_) => {
                Err(anyhow::anyhow!("OwnershipTransferred event"))
            }
        }
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
                    if attr.key_str().ok()? == cosmos_sdk::ATTRIBUTE_KEY_PACKET_DATA_HEX {
                        let packet: Vec<u8> = hex::decode(attr.value_str().ok()?).ok()?;
                        let packet = cosmos_sdk::proto::Packet::decode(packet.as_slice()).ok()?;
                        Some(Self::SendPacket(SendPacket {
                            packet: packet.try_into().ok()?,
                        }))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("No packet data found")),
            cosmos_sdk::EVENT_TYPE_RECV_PACKET => event
                .attributes
                .into_iter()
                .find_map(|attr| {
                    if attr.key_str().ok()? == cosmos_sdk::ATTRIBUTE_KEY_PACKET_DATA_HEX {
                        let packet: Vec<u8> = hex::decode(attr.value_str().ok()?).ok()?;
                        let packet = cosmos_sdk::proto::Packet::decode(packet.as_slice()).ok()?;
                        Some(Self::RecvPacket(RecvPacket {
                            packet: packet.try_into().ok()?,
                        }))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("No packet data found")),
            cosmos_sdk::EVENT_TYPE_ACKNOWLEDGE_PACKET => todo!(),
            cosmos_sdk::EVENT_TYPE_TIMEOUT_PACKET => todo!(),
            cosmos_sdk::EVENT_TYPE_WRITE_ACK => todo!(),
            _ => Err(anyhow::anyhow!("Unwanted event type: {}", event.kind)),
        }
    }
}
