//! ICS-25 Vector Commitments for IBC light clients on Solana
//!
//! This crate defines the standard interface that all IBC light clients must implement
//! to be compatible with the ICS-26 router for membership and non-membership verification.
//!
//! ICS-25 specifies the interface for vector commitment schemes used in IBC to verify
//! inclusion or non-inclusion of values at specific paths in a commitment root.

use anchor_lang::prelude::*;

// Include generated discriminators from build.rs
include!(concat!(env!("OUT_DIR"), "/discriminators.rs"));

/// Standard message structure for membership verification
/// All light clients must accept this structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipMsg {
    /// The height at which to verify
    pub height: u64,
    /// The merkle proof
    pub proof: Vec<u8>,
    /// The merkle path to the value
    pub path: Vec<Vec<u8>>,
    /// The value to verify
    pub value: Vec<u8>,
}

/// Standard message structure for non-membership verification
/// All light clients must accept this structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NonMembershipMsg {
    /// The height at which to verify
    pub height: u64,
    /// The merkle proof
    pub proof: Vec<u8>,
    /// The merkle path to the value
    pub path: Vec<Vec<u8>>,
}

/// Client status values returned by `client_status` instruction via `set_return_data`
pub mod client_status {
    pub const ACTIVE: u8 = 0;
    pub const FROZEN: u8 = 1;
    pub const EXPIRED: u8 = 2;
}
