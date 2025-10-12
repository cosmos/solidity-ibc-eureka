//! IBC types and utilities for Solana programs
//!
//! This crate provides all the types, messages, and utilities needed for
//! implementing IBC on Solana, including router messages (ICS26),
//! light client types (ICS07), and Solana-specific PDA utilities.

pub mod app_msgs;
pub mod events;
pub mod ics07;
pub mod pda;
pub mod router;

// Re-export commonly used types
pub use app_msgs::{
    IBCAppError, OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg, Payload,
};

pub use router::{
    IBCApp, MsgAckPacket, MsgCleanupChunks, MsgRecvPacket, MsgSendPacket, MsgTimeoutPacket,
    MsgUploadChunk, Packet, PayloadMetadata, ProofMetadata, CLIENT_SEED, CLIENT_SEQUENCE_SEED,
    COMMITMENT_SEED, IBC_APP_SEED, PACKET_ACK_SEED, PACKET_COMMITMENT_SEED, PACKET_RECEIPT_SEED,
    ROUTER_STATE_SEED,
};

pub use ics07::{
    ClientState, ConsensusState, IbcHeight, UpdateClientMsg, CONSENSUS_STATE_SEED,
    ICS07_INITIALIZE_DISCRIMINATOR,
};

pub use pda::*;

pub use events::{
    AckPacketEvent, ClientAddedEvent, ClientStatusUpdatedEvent, IBCAppAdded, NoopEvent,
    SendPacketEvent, TimeoutPacketEvent, WriteAcknowledgementEvent,
};
