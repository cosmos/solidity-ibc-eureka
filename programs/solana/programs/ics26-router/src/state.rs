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
#[derive(InitSpace)]
pub struct ClientSequence {
    /// Next sequence number for sending packets
    pub next_sequence_send: u64,
}

impl Default for ClientSequence {
    fn default() -> Self {
        Self {
            next_sequence_send: 1, // IBC sequences start from 1
        }
    }
}

/// Commitment storage (simple key-value)
#[account]
#[derive(InitSpace)]
pub struct Commitment {
    /// The commitment value (sha256 hash)
    pub value: [u8; 32],
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
