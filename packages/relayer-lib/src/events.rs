//! Define the events that can be retrieved by the relayer.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EurekaEvent {
    RecvPacket,
    AckPacket,
    TimeoutPacket,
    WriteAcknowledgement,
}
