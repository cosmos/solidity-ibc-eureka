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
    /// Authority that registered this client
    pub authority: Pubkey,
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
            authority: self.authority,
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

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    /// Ensures `IBCApp` in this program remains compatible with `solana_ibc_types::IBCApp`
    /// This is critical because the relayer deserializes on-chain `IBCApp` accounts
    /// using `solana_ibc_types::IBCApp`
    #[test]
    fn test_ibc_app_serialization_compatibility() {
        let app = IBCApp {
            version: AccountVersion::V1,
            port_id: "transfer".to_string(),
            app_program_id: Pubkey::new_unique(),
            authority: Pubkey::new_unique(),
            _reserved: [0; 256],
        };

        // Serialize the program's IBCApp
        // Note: try_to_vec() doesn't include discriminator - that's only added by Anchor
        // when writing to on-chain accounts
        let serialized = app.try_to_vec().unwrap();

        // Deserialize as solana_ibc_types::IBCApp to verify compatibility
        let types_app: solana_ibc_types::IBCApp =
            AnchorDeserialize::deserialize(&mut &serialized[..]).unwrap();

        // Verify all fields match
        assert_eq!(app.port_id, types_app.port_id);
        assert_eq!(app.app_program_id, types_app.app_program_id);
        assert_eq!(app.authority, types_app.authority);
        assert_eq!(app._reserved, types_app._reserved);
    }

    /// Ensures `Client` in this program remains compatible with marker type pattern
    #[test]
    fn test_client_seed_compatibility() {
        // Verify SEED constant matches between program and types
        assert_eq!(Client::SEED, solana_ibc_types::Client::SEED);
    }

    /// Ensures `RouterState` in this program remains compatible with marker type pattern
    #[test]
    fn test_router_state_seed_compatibility() {
        assert_eq!(RouterState::SEED, solana_ibc_types::RouterState::SEED);
    }

    /// Ensures `ClientSequence` in this program remains compatible with marker type pattern
    #[test]
    fn test_client_sequence_seed_compatibility() {
        assert_eq!(ClientSequence::SEED, solana_ibc_types::ClientSequence::SEED);
    }

    /// Ensures `Commitment` in this program remains compatible with marker type pattern
    #[test]
    fn test_commitment_seed_compatibility() {
        assert_eq!(Commitment::SEED, solana_ibc_types::Commitment::SEED);
        assert_eq!(
            Commitment::PACKET_COMMITMENT_SEED,
            solana_ibc_types::Commitment::PACKET_COMMITMENT_SEED
        );
        assert_eq!(
            Commitment::PACKET_RECEIPT_SEED,
            solana_ibc_types::Commitment::PACKET_RECEIPT_SEED
        );
        assert_eq!(
            Commitment::PACKET_ACK_SEED,
            solana_ibc_types::Commitment::PACKET_ACK_SEED
        );
    }

    /// Ensures `PayloadChunk` in this program remains compatible with marker type pattern
    #[test]
    fn test_payload_chunk_seed_compatibility() {
        assert_eq!(PayloadChunk::SEED, solana_ibc_types::PayloadChunk::SEED);
    }

    /// Ensures `ProofChunk` in this program remains compatible with marker type pattern
    #[test]
    fn test_proof_chunk_seed_compatibility() {
        assert_eq!(ProofChunk::SEED, solana_ibc_types::ProofChunk::SEED);
    }

    /// Ensures `AccountVersion` enum serialization remains compatible between program and types
    #[test]
    fn test_account_version_serialization_compatibility() {
        let version = AccountVersion::V1;

        let serialized = version.try_to_vec().unwrap();

        let types_version: solana_ibc_types::router::AccountVersion =
            AnchorDeserialize::deserialize(&mut &serialized[..]).unwrap();

        assert_eq!(
            version,
            match types_version {
                solana_ibc_types::router::AccountVersion::V1 => AccountVersion::V1,
            }
        );
    }
}
