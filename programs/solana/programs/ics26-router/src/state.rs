use anchor_lang::prelude::*;
use ics24_host_solana::Payload;

pub const MIN_PORT_ID_LENGTH: usize = 2;
pub const MAX_PORT_ID_LENGTH: usize = 128;
pub const MAX_CLIENT_ID_LENGTH: usize = 64;

/// Router state account
/// TODO: Implement multi-router ACL
#[account]
#[derive(InitSpace)]
pub struct RouterState {
    /// Authority that can perform restricted operations
    pub authority: Pubkey,
}

/// `IBCApp` mapping port IDs to IBC app program IDs
#[account]
#[derive(InitSpace)]
pub struct IBCApp {
    /// The port identifier
    #[max_len(MAX_PORT_ID_LENGTH)]
    pub port_id: String,
    /// The program ID of the IBC application
    pub app_program_id: Pubkey,
    /// Authority that registered this port
    pub authority: Pubkey,
}

/// Counterparty chain information
#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct CounterpartyInfo {
    /// Client ID on the counterparty chain
    #[max_len(MAX_CLIENT_ID_LENGTH)]
    pub client_id: String,
    /// Merkle prefix for proof verification
    #[max_len(8, 128)]
    pub merkle_prefix: Vec<Vec<u8>>,
}

/// Client mapping client IDs to light client program IDs
#[account]
#[derive(InitSpace)]
pub struct Client {
    /// The client identifier
    #[max_len(MAX_CLIENT_ID_LENGTH)]
    pub client_id: String,
    /// The program ID of the light client
    pub client_program_id: Pubkey,
    /// Counterparty chain information
    pub counterparty_info: CounterpartyInfo,
    /// Authority that registered this client
    pub authority: Pubkey,
    /// Whether the client is active
    pub active: bool,
}

/// Client sequence tracking
#[account]
#[derive(InitSpace, Default)]
pub struct ClientSequence {
    /// Next sequence number for sending packets
    pub next_sequence_send: u64,
}

/// Commitment storage (simple key-value)
#[account]
#[derive(InitSpace)]
pub struct Commitment {
    /// The commitment value (sha256 hash)
    pub value: [u8; 32],
}

/// Packet structure matching Ethereum's ICS26RouterMsgs.Packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Packet {
    pub sequence: u64,
    pub source_client: String,
    pub dest_client: String,
    pub timeout_timestamp: i64,
    pub payloads: Vec<Payload>,
}

/// Message structures for instructions
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MsgSendPacket {
    pub source_client: String,
    pub timeout_timestamp: i64,
    pub payload: Payload,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MsgRecvPacket {
    pub packet: Packet,
    pub proof_commitment: Vec<u8>,
    pub proof_height: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MsgAckPacket {
    pub packet: Packet,
    pub acknowledgement: Vec<u8>,
    pub proof_acked: Vec<u8>,
    pub proof_height: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MsgTimeoutPacket {
    pub packet: Packet,
    pub proof_timeout: Vec<u8>,
    pub proof_height: u64,
}

/// Constants
pub const ROUTER_STATE_SEED: &[u8] = b"router_state";
pub const IBC_APP_SEED: &[u8] = b"ibc_app";
pub const CLIENT_SEED: &[u8] = b"client";
pub const CLIENT_SEQUENCE_SEED: &[u8] = b"client_sequence";
pub const COMMITMENT_SEED: &[u8] = b"commitment";
pub const PACKET_COMMITMENT_SEED: &[u8] = b"packet_commitment";
pub const PACKET_RECEIPT_SEED: &[u8] = b"packet_receipt";
pub const PACKET_ACK_SEED: &[u8] = b"packet_ack";

/// Maximum timeout duration (1 day in seconds)
pub const MAX_TIMEOUT_DURATION: i64 = 86400;

#[event]
pub struct NoopEvent {}
