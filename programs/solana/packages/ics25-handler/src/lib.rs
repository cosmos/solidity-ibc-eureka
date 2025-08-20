//! ICS-25 Vector Commitments for IBC light clients on Solana
//!
//! This crate defines the standard interface that all IBC light clients must implement
//! to be compatible with the ICS-26 router for membership and non-membership verification.
//!
//! ICS-25 specifies the interface for vector commitment schemes used in IBC to verify
//! inclusion or non-inclusion of values at specific paths in a commitment root.

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

// Include the auto-generated discriminators for light client instructions
include!(concat!(env!("OUT_DIR"), "/discriminators.rs"));
