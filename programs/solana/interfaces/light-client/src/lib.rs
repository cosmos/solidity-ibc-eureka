//! Shared interface for IBC light client implementations on Solana
//!
//! This crate defines the standard interface that all IBC light clients must implement
//! to be compatible with the ICS26 router.

use anchor_lang::prelude::*;

/// Standard message structure for membership verification
/// All light clients must accept this structure for both membership and non-membership proofs
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipMsg {
    /// The height at which to verify
    pub height: u64,
    /// Delay time period (for time-based delays)
    pub delay_time_period: u64,
    /// Delay block period (for block-based delays)
    pub delay_block_period: u64,
    /// The merkle proof
    pub proof: Vec<u8>,
    /// The merkle path to the value
    pub path: Vec<Vec<u8>>,
    /// The value to verify (empty for non-membership)
    pub value: Vec<u8>,
}

// Include the auto-generated discriminators
include!(concat!(env!("OUT_DIR"), "/discriminators.rs"));
