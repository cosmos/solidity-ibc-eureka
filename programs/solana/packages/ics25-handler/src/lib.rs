//! ICS-25 Vector Commitments for IBC light clients on Solana
//!
//! This crate defines the standard interface that all IBC light clients must implement
//! to be compatible with the ICS-26 router for membership and non-membership verification.
//!
//! ICS-25 specifies the interface for vector commitment schemes used in IBC to verify
//! inclusion or non-inclusion of values at specific paths in a commitment root.

use anchor_lang::prelude::*;

pub trait ICS25Msg {
    /// The height at which to verify
    fn height(&self) -> u64;
    /// Delay time period (for time-based delays)
    fn delay_time_period(&self) -> u64;
    /// Delay block period (for block-based delays)
    fn delay_block_period(&self) -> u64;
}

/// Standard message structure for membership verification
/// All light clients must accept this structure
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
    /// The value to verify
    pub value: Vec<u8>,
}

/// Standard message structure for non-membership verification
/// All light clients must accept this structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NonMembershipMsg {
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
}

impl ICS25Msg for MembershipMsg {
    fn height(&self) -> u64 {
        self.height
    }

    fn delay_time_period(&self) -> u64 {
        self.delay_time_period
    }

    fn delay_block_period(&self) -> u64 {
        self.delay_block_period
    }
}

impl ICS25Msg for NonMembershipMsg {
    fn height(&self) -> u64 {
        self.height
    }

    fn delay_time_period(&self) -> u64 {
        self.delay_time_period
    }

    fn delay_block_period(&self) -> u64 {
        self.delay_block_period
    }
}

// Include the auto-generated discriminators for light client instructions
include!(concat!(env!("OUT_DIR"), "/discriminators.rs"));
