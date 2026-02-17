//! Router message types for IBC on Solana
//!
//! These types define the messages for packet handling in the ICS26 router.

use crate::Payload;
use anchor_lang::prelude::*;

// Import validation constant from solana-ibc-proto (single source of truth)
use solana_ibc_proto::MAX_CLIENT_ID_LENGTH;

/// ICS26 router instruction names and discriminators
pub mod router_instructions {
    use crate::utils::compute_discriminator;

    pub const RECV_PACKET: &str = "recv_packet";
    pub const ACK_PACKET: &str = "ack_packet";
    pub const TIMEOUT_PACKET: &str = "timeout_packet";
    pub const UPLOAD_PAYLOAD_CHUNK: &str = "upload_payload_chunk";
    pub const UPLOAD_PROOF_CHUNK: &str = "upload_proof_chunk";
    pub const CLEANUP_CHUNKS: &str = "cleanup_chunks";

    pub fn recv_packet_discriminator() -> [u8; 8] {
        compute_discriminator(RECV_PACKET)
    }

    pub fn ack_packet_discriminator() -> [u8; 8] {
        compute_discriminator(ACK_PACKET)
    }

    pub fn timeout_packet_discriminator() -> [u8; 8] {
        compute_discriminator(TIMEOUT_PACKET)
    }

    pub fn upload_payload_chunk_discriminator() -> [u8; 8] {
        compute_discriminator(UPLOAD_PAYLOAD_CHUNK)
    }

    pub fn upload_proof_chunk_discriminator() -> [u8; 8] {
        compute_discriminator(UPLOAD_PROOF_CHUNK)
    }

    pub fn cleanup_chunks_discriminator() -> [u8; 8] {
        compute_discriminator(CLEANUP_CHUNKS)
    }
}

/// Account schema version for upgradability
#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum AccountVersion {
    V1,
}

/// Counterparty chain information
#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct CounterpartyInfo {
    /// Client ID on the counterparty chain
    #[max_len(MAX_CLIENT_ID_LENGTH)]
    pub client_id: String,
    /// Merkle prefix for proof verification
    #[max_len(8, 128)]
    pub merkle_prefix: Vec<Vec<u8>>,
}

/// Client account structure mapping client IDs to light client program IDs
#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct ClientAccount {
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

/// Packet structure matching Ethereum's ICS26RouterMsgs.Packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct Packet {
    pub sequence: u64,
    pub source_client: String,
    pub dest_client: String,
    pub timeout_timestamp: i64,
    pub payloads: Vec<Payload>,
}

impl Packet {
    /// Returns the commitment path for the packet.
    /// Path format: sourceClient + 0x01 + sequence (big-endian)
    #[must_use]
    pub fn commitment_path(&self) -> Vec<u8> {
        let mut path = Vec::new();
        path.extend_from_slice(self.source_client.as_bytes());
        path.push(1_u8);
        path.extend_from_slice(&self.sequence.to_be_bytes());
        path
    }

    /// Returns the receipt commitment path for the packet.
    /// Path format: destClient + 0x02 + sequence (big-endian)
    #[must_use]
    pub fn receipt_commitment_path(&self) -> Vec<u8> {
        let mut path = Vec::new();
        path.extend_from_slice(self.dest_client.as_bytes());
        path.push(2_u8);
        path.extend_from_slice(&self.sequence.to_be_bytes());
        path
    }

    /// Returns the acknowledgment commitment path for the packet.
    /// Path format: destClient + 0x03 + sequence (big-endian)
    #[must_use]
    pub fn ack_commitment_path(&self) -> Vec<u8> {
        let mut path = Vec::new();
        path.extend_from_slice(self.dest_client.as_bytes());
        path.push(3_u8);
        path.extend_from_slice(&self.sequence.to_be_bytes());
        path
    }
}

/// Payload metadata for chunked operations
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PayloadMetadata {
    pub source_port: String,
    pub dest_port: String,
    pub version: String,
    pub encoding: String,
    pub total_chunks: u8,
}

/// Proof metadata for chunked operations
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ProofMetadata {
    pub height: u64,
    pub total_chunks: u8,
}

/// Message for sending a packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgSendPacket {
    pub source_client: String,
    pub timeout_timestamp: i64,
    pub payload: Payload,
}

/// Message for receiving a packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgRecvPacket {
    pub packet: Packet,
    pub payloads: Vec<PayloadMetadata>,
    pub proof: ProofMetadata,
}

/// Message for acknowledging a packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgAckPacket {
    pub packet: Packet,
    pub payloads: Vec<PayloadMetadata>,
    pub acknowledgement: Vec<u8>, // Not chunked
    pub proof: ProofMetadata,
}

/// Message for timing out a packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgTimeoutPacket {
    pub packet: Packet,
    pub payloads: Vec<PayloadMetadata>,
    pub proof: ProofMetadata,
}

/// Message for uploading chunks
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgUploadChunk {
    pub client_id: String,
    pub sequence: u64,
    pub payload_index: u8,
    pub chunk_index: u8,
    pub chunk_data: Vec<u8>,
}

/// Message for cleanup
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgCleanupChunks {
    pub client_id: String,
    pub sequence: u64,
    pub payload_chunks: Vec<u8>, // Number of chunks for each payload
    pub total_proof_chunks: u8,
}

/// `IBCApp` mapping port IDs to IBC app program IDs
/// This matches the on-chain account structure in the ICS26 router
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IBCApp {
    /// Schema version for upgrades
    pub version: AccountVersion,
    /// The port identifier
    pub port_id: String,
    /// The program ID of the IBC application
    pub app_program_id: Pubkey,
    /// Authority that registered this port
    pub authority: Pubkey,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

impl IBCApp {
    pub const SEED: &'static [u8] = b"ibc_app";

    /// Get IBC app PDA for a port
    pub fn pda(port_id: &str, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, port_id.as_bytes()], &program_id)
    }
}

/// Router state account - matches the on-chain account structure in ICS26 router
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct RouterState {
    /// Schema version for upgrades
    pub version: AccountVersion,
    /// Access manager program ID for role-based access control
    pub access_manager: Pubkey,
    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

impl RouterState {
    pub const SEED: &'static [u8] = b"router_state";

    /// Get router state PDA
    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
    }
}

pub struct Client;

impl Client {
    pub const SEED: &'static [u8] = b"client";

    /// Get client PDA
    pub fn pda(client_id: &str, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, client_id.as_bytes()], &program_id)
    }
}

pub struct ClientSequence;

impl ClientSequence {
    pub const SEED: &'static [u8] = b"client_sequence";

    /// Get client sequence PDA
    pub fn pda(client_id: &str, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, client_id.as_bytes()], &program_id)
    }
}

pub struct Commitment;

impl Commitment {
    pub const SEED: &'static [u8] = b"commitment";
    pub const PACKET_COMMITMENT_SEED: &'static [u8] = b"packet_commitment";
    pub const PACKET_RECEIPT_SEED: &'static [u8] = b"packet_receipt";
    pub const PACKET_ACK_SEED: &'static [u8] = b"packet_ack";

    /// Get packet commitment PDA
    pub fn packet_commitment_pda(
        client_id: &str,
        sequence: u64,
        program_id: Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::PACKET_COMMITMENT_SEED,
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
            ],
            &program_id,
        )
    }

    /// Get packet receipt PDA
    pub fn packet_receipt_pda(client_id: &str, sequence: u64, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::PACKET_RECEIPT_SEED,
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
            ],
            &program_id,
        )
    }

    /// Get packet acknowledgment PDA
    pub fn packet_ack_pda(client_id: &str, sequence: u64, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::PACKET_ACK_SEED,
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
            ],
            &program_id,
        )
    }
}

pub struct IBCAppState;

impl IBCAppState {
    pub const SEED: &'static [u8] = b"app_state";

    /// Get app state PDA for IBC applications
    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
    }
}

pub struct PayloadChunk;

impl PayloadChunk {
    pub const SEED: &'static [u8] = b"payload_chunk";

    /// Get payload chunk PDA
    #[allow(clippy::too_many_arguments)]
    pub fn pda(
        payer: Pubkey,
        client_id: &str,
        sequence: u64,
        payload_index: u8,
        chunk_index: u8,
        program_id: Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[payload_index],
                &[chunk_index],
            ],
            &program_id,
        )
    }
}

pub struct ProofChunk;

impl ProofChunk {
    pub const SEED: &'static [u8] = b"proof_chunk";

    /// Get proof chunk PDA
    pub fn pda(
        payer: Pubkey,
        client_id: &str,
        sequence: u64,
        chunk_index: u8,
        program_id: Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[chunk_index],
            ],
            &program_id,
        )
    }
}
