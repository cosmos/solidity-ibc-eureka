//! ICS07 Tendermint light client types for Solana
//!
//! These types define the messages and state for the ICS07 Tendermint light client.

use anchor_lang::prelude::*;

/// ICS07 consensus state PDA seed
pub const CONSENSUS_STATE_SEED: &[u8] = b"consensus_state";

/// Update client message for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct UpdateClientMsg {
    pub header: Vec<u8>, // Serialized Tendermint header
}

/// IBC height structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IbcHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

/// Trust level for Tendermint light client
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct TrustLevel {
    pub numerator: u64,
    pub denominator: u64,
}

/// Client state for ICS07 Tendermint
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

/// Consensus state for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ConsensusState {
    pub timestamp: i64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
}
