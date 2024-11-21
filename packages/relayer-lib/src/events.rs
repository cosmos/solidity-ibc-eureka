//! Define the events that can be retrieved by the relayer.

use alloy::sol_types::SolEvent;
use ibc_eureka_solidity_types::ics26::router::{
    routerEvents, AckPacket, RecvPacket, SendPacket, TimeoutPacket, WriteAcknowledgement,
};
use tendermint::abci::Event as TmEvent;

/// Events emitted by IBC Eureka implementations that the relayer is interested in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    /// Get the IBC provable path for the event.
    /// This is used to prove the event on the IBC chain.
    pub fn ibc_path(&self) -> Option<Vec<u8>> {
        match self {
            Self::SendPacket(e) => {
                // we need to append some bytes together
                let mut path = Vec::new();
                path.extend_from_slice(e.packet.sourceChannel.as_bytes());
                path.push(1_u8);
                path.extend_from_slice(&u64::from(e.packet.sequence).to_be_bytes());
                Some(path)
            }
            Self::WriteAcknowledgement(_) => todo!(),
            Self::TimeoutPacket(_) | Self::RecvPacket(_) | Self::AckPacket(_) => None,
        }
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

    fn try_from(_event: TmEvent) -> anyhow::Result<Self> {
        todo!()
    }
}
