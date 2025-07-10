use anchor_lang::prelude::*;

/// Router state account
#[account]
pub struct RouterState {
    /// Authority that can perform restricted operations
    pub authority: Pubkey,
    /// Whether the router is initialized
    pub initialized: bool,
}

/// Port registry mapping port IDs to IBC app program IDs
#[account]
pub struct PortRegistry {
    /// The port identifier
    pub port_id: String,
    /// The program ID of the IBC application
    pub app_program_id: Pubkey,
    /// Authority that registered this port
    pub authority: Pubkey,
}

/// Client sequence tracking
#[account]
pub struct ClientSequence {
    /// The client identifier
    pub client_id: String,
    /// Next sequence number for sending packets
    pub next_sequence_send: u64,
    /// Next sequence number for receiving packets
    pub next_sequence_recv: u64,
    /// Next sequence number for acknowledgments
    pub next_sequence_ack: u64,
}

/// Commitment storage (simple key-value)
#[account]
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

/// Payload structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Payload {
    pub source_port: String,
    pub dest_port: String,
    pub version: String,
    pub encoding: String,
    pub value: Vec<u8>,
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
pub const PORT_REGISTRY_SEED: &[u8] = b"port_registry";
pub const CLIENT_SEQUENCE_SEED: &[u8] = b"client_sequence";
pub const COMMITMENT_SEED: &[u8] = b"commitment";
pub const PACKET_COMMITMENT_SEED: &[u8] = b"packet_commitment";
pub const PACKET_RECEIPT_SEED: &[u8] = b"packet_receipt";
pub const PACKET_ACK_SEED: &[u8] = b"packet_ack";

/// Maximum timeout duration (1 day in seconds)
pub const MAX_TIMEOUT_DURATION: i64 = 86400;