//! PDA (Program Derived Address) derivation functions for IBC on Solana
//!
//! These functions help derive the PDAs used by the ICS26 router and ICS07 light client.

use crate::ics07::CONSENSUS_STATE_SEED;
use crate::router::*;
use anchor_lang::prelude::*;

// Use default program IDs from the constants crate
// These can be overridden by passing explicit program IDs to the functions
use core::str::FromStr;
use solana_ibc_constants::{ICS07_TENDERMINT_ID, ICS26_ROUTER_ID};

/// Derive router state PDA
pub fn derive_router_state(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ROUTER_STATE_SEED], program_id)
}

/// Derive IBC app PDA for a port
pub fn derive_ibc_app(port_id: &str, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[IBC_APP_SEED, port_id.as_bytes()], program_id)
}

/// Derive client PDA
pub fn derive_client(client_id: &str, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CLIENT_SEED, client_id.as_bytes()], program_id)
}

/// Derive client sequence PDA
pub fn derive_client_sequence(client_id: &str, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CLIENT_SEQUENCE_SEED, client_id.as_bytes()], program_id)
}

/// Derive packet commitment PDA
pub fn derive_packet_commitment(
    client_id: &str,
    sequence: u64,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PACKET_COMMITMENT_SEED,
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        program_id,
    )
}

/// Derive packet receipt PDA
pub fn derive_packet_receipt(client_id: &str, sequence: u64, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PACKET_RECEIPT_SEED,
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        program_id,
    )
}

/// Derive packet acknowledgment PDA
pub fn derive_packet_ack(client_id: &str, sequence: u64, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PACKET_ACK_SEED,
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        program_id,
    )
}

/// Derive app state PDA for IBC applications
pub fn derive_app_state(port_id: &str, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[APP_STATE_SEED, port_id.as_bytes()], program_id)
}

/// Derive ICS07 client state PDA
pub fn derive_ics07_client_state(chain_id: &str, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], program_id)
}

/// Derive ICS07 consensus state PDA
pub fn derive_ics07_consensus_state(
    client_state: &Pubkey,
    height: u64,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            CONSENSUS_STATE_SEED,
            client_state.as_ref(),
            &height.to_le_bytes(),
        ],
        program_id,
    )
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

// Convenience functions that use default program IDs
// These can be used when the standard deployed program IDs are being used

/// Derive router state PDA using the default ICS26 program ID
pub fn derive_router_state_default() -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(ICS26_ROUTER_ID).expect("Invalid ICS26 program ID");
    derive_router_state(&program_id)
}

/// Derive IBC app PDA using the default ICS26 program ID
pub fn derive_ibc_app_default(port_id: &str) -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(ICS26_ROUTER_ID).expect("Invalid ICS26 program ID");
    derive_ibc_app(port_id, &program_id)
}

/// Derive client PDA using the default ICS26 program ID
pub fn derive_client_default(client_id: &str) -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(ICS26_ROUTER_ID).expect("Invalid ICS26 program ID");
    derive_client(client_id, &program_id)
}

/// Derive ICS07 client state PDA using the default ICS07 program ID
pub fn derive_ics07_client_state_default(chain_id: &str) -> (Pubkey, u8) {
    let program_id = Pubkey::from_str(ICS07_TENDERMINT_ID).expect("Invalid ICS07 program ID");
    derive_ics07_client_state(chain_id, &program_id)
}
