//! Anchor types and PDA derivation functions for ICS26 Router and ICS07 Tendermint

use anchor_lang::prelude::*;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// ICS26 Router Program ID
pub const ICS26_ROUTER_ID: &str = "FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx";

/// ICS07 Tendermint Program ID
pub const ICS07_TENDERMINT_ID: &str = "HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD";

/// Get the ICS26 Router program ID
pub fn ics26_program_id() -> Pubkey {
    Pubkey::from_str(ICS26_ROUTER_ID).expect("Invalid ICS26 program ID")
}

/// Get the ICS07 Tendermint program ID
pub fn ics07_program_id() -> Pubkey {
    Pubkey::from_str(ICS07_TENDERMINT_ID).expect("Invalid ICS07 program ID")
}

// Seed constants from the Solana programs
pub const ROUTER_STATE_SEED: &[u8] = b"router_state";
pub const IBC_APP_SEED: &[u8] = b"ibc_app";
pub const CLIENT_SEED: &[u8] = b"client";
pub const CLIENT_SEQUENCE_SEED: &[u8] = b"client_sequence";
pub const COMMITMENT_SEED: &[u8] = b"commitment";
pub const PACKET_COMMITMENT_SEED: &[u8] = b"packet_commitment";
pub const PACKET_RECEIPT_SEED: &[u8] = b"packet_receipt";
pub const PACKET_ACK_SEED: &[u8] = b"packet_ack";

// ICS07 seeds
pub const CONSENSUS_STATE_SEED: &[u8] = b"consensus_state";

/// Packet structure matching the Anchor program
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct Packet {
    pub sequence: u64,
    pub source_client: String,
    pub dest_client: String,
    pub timeout_timestamp: i64,
    pub payloads: Vec<Payload>,
}

/// Payload structure matching the Anchor program
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct Payload {
    pub source_port: String,
    pub dest_port: String,
    pub version: String,
    pub encoding: String,
    pub value: Vec<u8>,
}

/// Message structures for ICS26 Router instructions
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgSendPacket {
    pub source_client: String,
    pub timeout_timestamp: i64,
    pub payload: Payload,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgRecvPacket {
    pub packet: Packet,
    pub proof_commitment: Vec<u8>,
    pub proof_height: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgAckPacket {
    pub packet: Packet,
    pub acknowledgement: Vec<u8>,
    pub proof_acked: Vec<u8>,
    pub proof_height: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MsgTimeoutPacket {
    pub packet: Packet,
    pub proof_timeout: Vec<u8>,
    pub proof_height: u64,
}

/// Derive router state PDA
pub fn derive_router_state() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ROUTER_STATE_SEED], &ics26_program_id())
}

/// Derive IBC app PDA for a port
pub fn derive_ibc_app(port_id: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[IBC_APP_SEED, port_id.as_bytes()], &ics26_program_id())
}

/// Derive client PDA
pub fn derive_client(client_id: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CLIENT_SEED, client_id.as_bytes()], &ics26_program_id())
}

/// Derive client sequence PDA
pub fn derive_client_sequence(client_id: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[CLIENT_SEQUENCE_SEED, client_id.as_bytes()],
        &ics26_program_id(),
    )
}

/// Derive packet commitment PDA
pub fn derive_packet_commitment(client_id: &str, sequence: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PACKET_COMMITMENT_SEED,
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ics26_program_id(),
    )
}

/// Derive packet receipt PDA
pub fn derive_packet_receipt(client_id: &str, sequence: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PACKET_RECEIPT_SEED,
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ics26_program_id(),
    )
}

/// Derive packet acknowledgment PDA
pub fn derive_packet_ack(client_id: &str, sequence: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PACKET_ACK_SEED,
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ics26_program_id(),
    )
}

/// Derive ICS07 client state PDA
pub fn derive_ics07_client_state(chain_id: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], &ics07_program_id())
}

/// Derive ICS07 consensus state PDA
pub fn derive_ics07_consensus_state(client_state: &Pubkey, height: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            CONSENSUS_STATE_SEED,
            client_state.as_ref(),
            &height.to_le_bytes(),
        ],
        &ics07_program_id(),
    )
}

/// ICS07 Tendermint types
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct UpdateClientMsg {
    pub header: Vec<u8>, // Serialized Tendermint header
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ClientState {
    pub chain_id: String,
    pub trust_level: TrustLevel,
    pub trusting_period: i64,
    pub unbonding_period: i64,
    pub max_clock_drift: i64,
    pub frozen_height: Option<IbcHeight>,
    pub latest_height: IbcHeight,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct TrustLevel {
    pub numerator: u64,
    pub denominator: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IbcHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ConsensusState {
    pub timestamp: i64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
}

/// Build instruction discriminator for Anchor
pub fn get_instruction_discriminator(instruction_name: &str) -> [u8; 8] {
    let preimage = format!("global:{}", instruction_name);
    let mut hash = [0u8; 8];
    hash.copy_from_slice(
        &anchor_lang::solana_program::hash::hash(preimage.as_bytes()).to_bytes()[..8],
    );
    hash
}

