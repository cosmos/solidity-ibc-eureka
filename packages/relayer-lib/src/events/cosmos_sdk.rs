//! Contains the types needed to parse Cosmos SDK's IBC Eureka events.
//!
//! Should be kept in sync with
//! <https://github.com/cosmos/ibc-go/blob/13a13abea09415f2d5c2b4c4ac8edf6b756b8e74/modules/core/04-channel/v2/types/events.go#L9>.

/// The event type for a send packet event.
pub const EVENT_TYPE_SEND_PACKET: &str = "send_packet";
/// The event type for a receive packet event.
pub const EVENT_TYPE_RECV_PACKET: &str = "recv_packet";
/// The event type for a timeout packet event.
pub const EVENT_TYPE_TIMEOUT_PACKET: &str = "timeout_packet";
/// The event type for an acknowledge packet event.
pub const EVENT_TYPE_ACKNOWLEDGE_PACKET: &str = "acknowledge_packet";
/// The event type for a write acknowledgement event.
pub const EVENT_TYPE_WRITE_ACK: &str = "write_acknowledgement";

/// The attribute key for the channel ID.
pub const ATTRIBUTE_KEY_CHANNEL_ID: &str = "channel_id";
/// The attribute key for the client ID.
pub const ATTRIBUTE_KEY_CLIENT_ID: &str = "client_id";
/// The attribute key for the counterparty channel ID.
pub const ATTRIBUTE_KEY_COUNTERPARTY_CHANNEL_ID: &str = "counterparty_channel_id";
/// The attribute key for the source channel.
pub const ATTRIBUTE_KEY_SRC_CHANNEL: &str = "packet_source_channel";
/// The attribute key for the destination channel.
pub const ATTRIBUTE_KEY_DST_CHANNEL: &str = "packet_dest_channel";
/// The attribute key for the sequence.
pub const ATTRIBUTE_KEY_SEQUENCE: &str = "packet_sequence";
/// The attribute key for the timeout timestamp.
pub const ATTRIBUTE_KEY_TIMEOUT_TIMESTAMP: &str = "packet_timeout_timestamp";
/// The attribute key for the packet data hex.
pub const ATTRIBUTE_KEY_PACKET_DATA_HEX: &str = "packet_data_hex";
/// The attribute key for the acknowledgement data hex.
pub const ATTRIBUTE_KEY_ACK_DATA_HEX: &str = "acknowledgement_data_hex";

/// Defines the protobuf types needed to parse IBC Eureka events.
// TODO: Replace with ibc-proto once it's available, or use autogenerated types.
pub mod proto {
    #![allow(missing_docs)]

    use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::{
        Packet as SolPacket, Payload as SolPayload,
    };

    /// IBC Eureka packet.
    #[derive(::prost::Message)]
    pub struct Packet {
        #[prost(uint64, tag = "1")]
        pub sequence: u64,
        #[prost(string, tag = "2")]
        pub source_channel: String,
        #[prost(string, tag = "3")]
        pub destination_channel: String,
        #[prost(uint64, tag = "4")]
        pub timeout_timestamp: u64,
        #[prost(message, repeated, tag = "5")]
        pub payloads: ::prost::alloc::vec::Vec<Payload>,
    }

    /// IBC Eureka acknowledgement.
    #[derive(::prost::Message)]
    pub struct Acknowledgement {
        #[prost(bytes = "vec", repeated, tag = "1")]
        pub app_acknowledgements: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    }

    /// Packet payload.
    #[derive(::prost::Message)]
    pub struct Payload {
        #[prost(string, tag = "1")]
        pub source_port: String,
        #[prost(string, tag = "2")]
        pub destination_port: String,
        #[prost(string, tag = "3")]
        pub version: String,
        #[prost(string, tag = "4")]
        pub encoding: String,
        #[prost(bytes = "vec", tag = "5")]
        pub value: ::prost::alloc::vec::Vec<u8>,
    }

    impl TryFrom<Packet> for SolPacket {
        type Error = <u64 as TryInto<u32>>::Error;

        fn try_from(packet: Packet) -> Result<Self, Self::Error> {
            Ok(Self {
                sequence: packet.sequence.try_into()?,
                sourceChannel: packet.source_channel,
                destChannel: packet.destination_channel,
                timeoutTimestamp: packet.timeout_timestamp,
                payloads: packet.payloads.into_iter().map(Into::into).collect(),
            })
        }
    }

    impl From<Payload> for SolPayload {
        fn from(payload: Payload) -> Self {
            Self {
                sourcePort: payload.source_port,
                destPort: payload.destination_port,
                version: payload.version,
                encoding: payload.encoding,
                value: payload.value.into(),
            }
        }
    }
}