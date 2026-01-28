//! IBC types and utilities for Solana programs
//!
//! This crate provides all the types, messages, and utilities needed for
//! implementing IBC on Solana, including router messages (ICS26),
//! light client types (ICS07), and Solana-specific PDA utilities.

pub mod access_manager;
pub mod app_msgs;
pub mod borsh_header;
pub mod cpi;
pub mod events;
pub mod ibc_app;
pub mod ics07;
pub mod ics27;
pub mod router;
pub mod utils;

// Re-export commonly used types
pub use app_msgs::{
    IBCAppError, OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg, Payload,
};

pub use router::{
    router_instructions, AccountVersion, Client, ClientAccount, ClientSequence, Commitment,
    CounterpartyInfo, IBCApp, IBCAppState, MsgAckPacket, MsgCleanupChunks, MsgRecvPacket,
    MsgSendPacket, MsgTimeoutPacket, MsgUploadChunk, Packet, PayloadChunk, PayloadMetadata,
    ProofChunk, ProofMetadata, RouterState,
};

// Re-export MAX_CLIENT_ID_LENGTH from solana-ibc-proto (single source of truth)
pub use solana_ibc_proto::MAX_CLIENT_ID_LENGTH;

pub use ics07::{
    ics07_instructions, ClientState, ConsensusState, IbcHeight, UpdateClientMsg,
    ASSEMBLE_UPDATE_CLIENT_STATIC_ACCOUNTS,
};

pub use ics27::{
    CallResultStatus, ClientId, ConstrainedBytes, ConstrainedError, ConstrainedString,
    ConstrainedVec, GMPAccount, GMPAppState, GMPCallResult, GMPPacketError, GmpPacketData, Salt,
    SignerSeeds, MAX_MEMO_LENGTH, MAX_RECEIVER_LENGTH, MAX_SALT_LENGTH, MAX_SENDER_LENGTH,
};

pub use events::{
    AccessManagerUpdated, AckPacketEvent, ClientAddedEvent, ClientUpdatedEvent, IBCAppAdded,
    NoopEvent, SendPacketEvent, TimeoutPacketEvent, WriteAcknowledgementEvent,
};

pub use access_manager::{roles, AccessManager};
pub use cpi::{
    reject_cpi, require_direct_call_or_whitelisted_caller, validate_cpi_caller, CpiValidationError,
};
pub use utils::compute_discriminator;
