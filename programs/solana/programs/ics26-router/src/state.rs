use anchor_lang::prelude::*;

// Re-export types from solana_ibc_types for use in instructions
pub use solana_ibc_types::{
    MsgAckPacket, MsgCleanupChunks, MsgRecvPacket, MsgSendPacket, MsgTimeoutPacket, MsgUploadChunk,
    Packet, PayloadMetadata, ProofMetadata,
};
pub use solana_ibc_types::{CLIENT_SEED, CLIENT_SEQUENCE_SEED, IBC_APP_SEED, ROUTER_STATE_SEED};
pub use solana_ibc_types::{
    COMMITMENT_SEED, PACKET_ACK_SEED, PACKET_COMMITMENT_SEED, PACKET_RECEIPT_SEED,
};

// PDA seeds for chunks
pub const PAYLOAD_CHUNK_SEED: &[u8] = b"payload_chunk";
pub const PROOF_CHUNK_SEED: &[u8] = b"proof_chunk";

pub const MIN_PORT_ID_LENGTH: usize = 2;
pub const MAX_PORT_ID_LENGTH: usize = 128;
pub const MAX_CLIENT_ID_LENGTH: usize = 64;

/// Account schema version
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum AccountVersion {
    V1,
}

/// Router state account
/// TODO: Implement multi-router ACL
#[account]
#[derive(InitSpace)]
pub struct RouterState {
    /// Schema version for upgrades
    pub version: AccountVersion,
    /// Authority that can perform restricted operations
    pub authority: Pubkey,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

/// `IBCApp` mapping port IDs to IBC app program IDs
#[account]
#[derive(InitSpace)]
pub struct IBCApp {
    /// Schema version for upgrades
    pub version: AccountVersion,
    /// The port identifier
    #[max_len(MAX_PORT_ID_LENGTH)]
    pub port_id: String,
    /// The program ID of the IBC application
    pub app_program_id: Pubkey,
    /// Authority that registered this port
    pub authority: Pubkey,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
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
    /// Schema version for upgrades
    pub version: AccountVersion,
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
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

/// Client sequence tracking
#[account]
#[derive(InitSpace)]
pub struct ClientSequence {
    /// Schema version for upgrades
    pub version: AccountVersion,
    /// Next sequence number for sending packets
    pub next_sequence_send: u64,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

impl Default for ClientSequence {
    fn default() -> Self {
        Self {
            next_sequence_send: 1, // IBC sequences start from 1
            version: AccountVersion::V1,
            _reserved: [0; 256],
        }
    }
}

/// Commitment storage (simple key-value)
#[account]
#[derive(InitSpace)]
pub struct Commitment {
    /// The commitment value (sha256 hash)
    pub value: [u8; 32],
    /// Timestamp when the commitment was created
    pub created_at: i64,
}

// Types are now imported from solana_ibc_types

/// Maximum timeout duration (1 day in seconds)
pub const MAX_TIMEOUT_DURATION: i64 = 86400;

/// Maximum size of chunk data
pub const CHUNK_DATA_SIZE: usize = 700;

/// Storage for payload chunks during multi-transaction upload
#[account]
#[derive(InitSpace)]
pub struct PayloadChunk {
    /// Client ID this chunk belongs to
    #[max_len(MAX_CLIENT_ID_LENGTH)]
    pub client_id: String,
    /// Packet sequence number
    pub sequence: u64,
    /// Index of the payload this chunk belongs to (for multi-payload packets)
    pub payload_index: u8,
    /// Index of this chunk (0-based)
    pub chunk_index: u8,
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}

/// Storage for proof chunks during multi-transaction upload
#[account]
#[derive(InitSpace)]
pub struct ProofChunk {
    /// Client ID this chunk belongs to
    #[max_len(MAX_CLIENT_ID_LENGTH)]
    pub client_id: String,
    /// Packet sequence number
    pub sequence: u64,
    /// Index of this chunk (0-based)
    pub chunk_index: u8,
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}
