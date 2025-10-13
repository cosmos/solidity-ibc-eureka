//! Router message types for IBC on Solana
//!
//! These types define the messages for packet handling in the ICS26 router.

use crate::Payload;
use anchor_lang::prelude::*;

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
    pub commitment: [u8; 32],
    pub total_chunks: u8,
}

/// Proof metadata for chunked operations
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ProofMetadata {
    pub height: u64,
    pub commitment: [u8; 32],
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

/// Message for acknowledging a packet - updated for chunking with multi-payload support
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgAckPacket {
    pub packet: Packet,
    pub payloads: Vec<PayloadMetadata>,
    pub acknowledgement: Vec<u8>, // Not chunked
    pub proof: ProofMetadata,
}

/// Message for timing out a packet - updated for chunking with multi-payload support
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
    pub payload_index: u8, // Which payload this chunk belongs to (for multi-payload support)
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

/// IBCApp mapping port IDs to IBC app program IDs
/// This matches the on-chain account structure in the ICS26 router
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IBCApp {
    /// The port identifier
    pub port_id: String,
    /// The program ID of the IBC application
    pub app_program_id: Pubkey,
    /// Authority that registered this port
    pub authority: Pubkey,
}

/// PDA seed constants for the ICS26 router
pub const ROUTER_STATE_SEED: &[u8] = b"router_state";
pub const IBC_APP_SEED: &[u8] = b"ibc_app";
pub const CLIENT_SEED: &[u8] = b"client";
pub const CLIENT_SEQUENCE_SEED: &[u8] = b"client_sequence";
pub const COMMITMENT_SEED: &[u8] = b"commitment";
pub const PACKET_COMMITMENT_SEED: &[u8] = b"packet_commitment";
pub const PACKET_RECEIPT_SEED: &[u8] = b"packet_receipt";
pub const PACKET_ACK_SEED: &[u8] = b"packet_ack";
pub const APP_STATE_SEED: &[u8] = b"app_state";
