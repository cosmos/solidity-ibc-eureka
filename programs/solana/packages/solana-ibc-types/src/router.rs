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
    pub proof_commitment: Vec<u8>,
    pub proof_height: u64,
}

/// Message for acknowledging a packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgAckPacket {
    pub packet: Packet,
    pub acknowledgement: Vec<u8>,
    pub proof_acked: Vec<u8>,
    pub proof_height: u64,
}

/// Message for timing out a packet
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgTimeoutPacket {
    pub packet: Packet,
    pub proof_timeout: Vec<u8>,
    pub proof_height: u64,
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
