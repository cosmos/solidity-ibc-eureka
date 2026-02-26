use anchor_lang::prelude::*;

// Re-export types from solana_ibc_types for use in instructions
pub use solana_ibc_types::{
    AccountVersion, ClientAccount, CounterpartyInfo, MsgAckPacket, MsgCleanupChunks, MsgRecvPacket,
    MsgSendPacket, MsgTimeoutPacket, MsgUploadChunk, Packet, PayloadMetadata, ProofMetadata,
    MAX_CLIENT_ID_LENGTH,
};

pub const MIN_PORT_ID_LENGTH: usize = 2;
pub const MAX_PORT_ID_LENGTH: usize = 128;

/// Global ICS26 router configuration.
///
/// Singleton PDA initialized once during program setup. Stores the link
/// to the access manager for admin-gated operations (e.g. registering
/// clients, migrating light clients) and a schema version for future
/// on-chain migrations.
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

/// Port-to-program mapping for IBC applications.
///
/// Each registered IBC application (e.g. ICS20 transfer, ICS27 GMP) gets
/// one `IBCApp` PDA derived from its port ID. The router uses this account
/// to look up which program to CPI into when delivering a received packet
/// or forwarding an acknowledgement/timeout to the application layer.
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

/// Client-ID-to-light-client mapping with counterparty chain metadata.
///
/// Created when an admin registers a new IBC client (e.g. an ICS07
/// Tendermint or attestation light client). The router reads this
/// account during `send_packet`, `recv_packet`, `ack_packet` and
/// `timeout_packet` to resolve which light client program to call for
/// proof verification, and to obtain the counterparty chain's client
/// and Merkle prefix information.
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

/// Per-client packet sequence counter.
///
/// Tracks the next sequence number to assign when sending a packet
/// through a given client. Each `send_packet` call reads and increments
/// this value to guarantee unique, monotonically increasing sequence
/// numbers for replay protection.
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

/// IBC packet commitment, receipt, or acknowledgement hash.
///
/// A generic 32-byte hash PDA used for three purposes depending on its
/// seed prefix:
/// - **Packet commitment** (`PACKET_COMMITMENT_SEED`): written by
///   `send_packet`, stores `sha256(packet)` so the counterparty can
///   prove the packet was sent.
/// - **Packet receipt** (`PACKET_RECEIPT_SEED`): written by
///   `recv_packet`, prevents the same packet from being delivered twice.
/// - **Packet acknowledgement** (`PACKET_ACK_SEED`): written by
///   `recv_packet`, stores the app-layer acknowledgement hash so the
///   sender chain can confirm delivery.
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

/// Temporary storage for a single chunk of IBC packet payload data during
/// multi-transaction upload.
///
/// Large payloads that exceed the Solana transaction size limit are split
/// into chunks and uploaded separately. The `recv_packet` instruction
/// reassembles all chunks, processes the full payload, then closes these
/// accounts to reclaim rent.
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

/// Temporary storage for a single chunk of IBC membership proof data
/// during multi-transaction upload.
///
/// Membership proofs (e.g. Merkle proofs or attestation signatures) can
/// exceed the Solana transaction size limit. They are uploaded in chunks
/// and reassembled when the final `recv_packet`, `ack_packet`, or
/// `timeout_packet` instruction executes verification.
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

    /// Ensures `RouterState` in this program remains compatible with `solana_ibc_types::RouterState`
    /// This is critical because the relayer deserializes on-chain `RouterState` accounts
    /// using `solana_ibc_types::RouterState`
    #[test]
    fn test_router_state_serialization_compatibility() {
        let router_state = RouterState {
            version: AccountVersion::V1,
            access_manager: Pubkey::new_unique(),
            _reserved: [0; 256],
        };

        // Serialize the program's RouterState
        // Note: try_to_vec() doesn't include discriminator - that's only added by Anchor
        // when writing to on-chain accounts
        let serialized = router_state.try_to_vec().unwrap();

        // Deserialize as solana_ibc_types::RouterState to verify compatibility
        let types_router_state: solana_ibc_types::RouterState =
            AnchorDeserialize::deserialize(&mut &serialized[..]).unwrap();

        // Verify all fields match
        assert_eq!(
            router_state.access_manager,
            types_router_state.access_manager
        );
        assert_eq!(router_state._reserved, types_router_state._reserved);

        // Verify SEED constant matches
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
