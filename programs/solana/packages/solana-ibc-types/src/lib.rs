//! IBC types and utilities for Solana programs
//!
//! This crate provides all the types, messages, and utilities needed for
//! implementing IBC on Solana, including router messages (ICS26),
//! light client types (ICS07), and Solana-specific PDA utilities.

pub mod app_msgs;
pub mod events;
pub mod ibc_app_interface;
pub mod ics07;
pub mod ics27;
pub mod router;

// Re-export commonly used types
pub use app_msgs::{
    IBCAppError, OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg, Payload,
};

pub use router::{
    router_instructions, Client, ClientSequence, Commitment, IBCApp, IBCAppState, MsgAckPacket,
    MsgCleanupChunks, MsgRecvPacket, MsgSendPacket, MsgTimeoutPacket, MsgUploadChunk, Packet,
    PayloadChunk, PayloadMetadata, ProofChunk, ProofMetadata, RouterState,
};

pub use ics07::{ics07_instructions, ClientState, ConsensusState, IbcHeight, UpdateClientMsg};

pub use ics27::{GMPAppState, GmpAccountState};

pub use events::{
    AckPacketEvent, ClientAddedEvent, ClientStatusUpdatedEvent, IBCAppAdded, NoopEvent,
    SendPacketEvent, TimeoutPacketEvent, WriteAcknowledgementEvent,
};

pub use ibc_app_interface::ibc_app_instructions;
