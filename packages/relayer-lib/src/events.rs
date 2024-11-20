//! Define the events that can be retrieved by the relayer.

use ibc_eureka_solidity_types::ics26::router::{
    AckPacket, RecvPacket, SendPacket, TimeoutPacket, WriteAcknowledgement,
};

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
