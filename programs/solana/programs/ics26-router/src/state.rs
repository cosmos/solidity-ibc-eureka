use anchor_lang::prelude::*;

// Re-export types from solana_ibc_types for use in instructions
pub use solana_ibc_types::{
    AccountVersion, ClientAccount, CounterpartyInfo, MsgAckPacket, MsgCleanupChunks, MsgRecvPacket,
    MsgSendPacket, MsgTimeoutPacket, MsgUploadChunk, Packet, PayloadMetadata, ProofMetadata,
    MAX_CLIENT_ID_LENGTH,
};

pub const MIN_PORT_ID_LENGTH: usize = 2;
pub const MAX_PORT_ID_LENGTH: usize = 128;

/// Router state account
#[account]
#[derive(InitSpace)]
pub struct RouterState {
    /// Schema version for upgrades
    pub version: AccountVersion,
    /// Access manager program ID for role-based access control
    pub access_manager: Pubkey,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

impl RouterState {
    pub const SEED: &'static [u8] = solana_ibc_types::RouterState::SEED;
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

impl IBCApp {
    pub const SEED: &'static [u8] = solana_ibc_types::router::IBCApp::SEED;
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
    /// Whether the client is active
    pub active: bool,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

impl Client {
    pub const SEED: &'static [u8] = solana_ibc_types::Client::SEED;

    /// Convert to `ClientAccount` for event emission
    pub fn to_client_account(&self) -> ClientAccount {
        ClientAccount {
            version: self.version,
            client_id: self.client_id.clone(),
            client_program_id: self.client_program_id,
            counterparty_info: self.counterparty_info.clone(),
            active: self.active,
            _reserved: self._reserved,
        }
    }
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

impl ClientSequence {
    pub const SEED: &'static [u8] = solana_ibc_types::ClientSequence::SEED;
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
}

impl Commitment {
    pub const SEED: &'static [u8] = solana_ibc_types::Commitment::SEED;
    pub const PACKET_COMMITMENT_SEED: &'static [u8] =
        solana_ibc_types::Commitment::PACKET_COMMITMENT_SEED;
    pub const PACKET_RECEIPT_SEED: &'static [u8] =
        solana_ibc_types::Commitment::PACKET_RECEIPT_SEED;
    pub const PACKET_ACK_SEED: &'static [u8] = solana_ibc_types::Commitment::PACKET_ACK_SEED;

    /// Empty commitment value (used to detect newly initialized accounts)
    pub const EMPTY: [u8; 32] = [0; 32];
}

/// Maximum timeout duration (1 day in seconds)
pub const MAX_TIMEOUT_DURATION: i64 = 86400;

pub use solana_ibc_constants::CHUNK_DATA_SIZE;

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

impl PayloadChunk {
    pub const SEED: &'static [u8] = solana_ibc_types::PayloadChunk::SEED;
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

impl ProofChunk {
    pub const SEED: &'static [u8] = solana_ibc_types::ProofChunk::SEED;
}
