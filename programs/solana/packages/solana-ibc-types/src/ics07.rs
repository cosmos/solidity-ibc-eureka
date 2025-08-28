//! ICS07 Tendermint light client types for Solana
//!
//! These types define the messages and state for the ICS07 Tendermint light client.

use anchor_lang::prelude::*;

/// ICS07 consensus state PDA seed
pub const CONSENSUS_STATE_SEED: &[u8] = b"consensus_state";

/// ICS07 initialize instruction discriminator
/// This is computed as the first 8 bytes of SHA256("global:initialize")
/// Following Anchor's discriminator calculation formula
pub const ICS07_INITIALIZE_DISCRIMINATOR: [u8; 8] = [175, 175, 109, 31, 13, 152, 155, 237];

/// Update client message for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct UpdateClientMsg {
    pub client_message: Vec<u8>, // Serialized Tendermint header
}

/// IBC height structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IbcHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

/// Client state for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ClientState {
    pub chain_id: String,
    pub trust_level_numerator: u64,
    pub trust_level_denominator: u64,
    pub trusting_period: u64,
    pub unbonding_period: u64,
    pub max_clock_drift: u64,
    pub frozen_height: IbcHeight,
    pub latest_height: IbcHeight,
}

/// Consensus state for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ConsensusState {
    pub timestamp: u64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
}
