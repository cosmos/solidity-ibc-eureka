use anchor_lang::prelude::*;

use crate::ETH_ADDRESS_LEN;

/// Emitted when conflicting timestamps are submitted for the same height.
#[event]
#[derive(Debug, Clone)]
pub struct MisbehaviourDetected {
    pub height: u64,
    pub existing_timestamp: u64,
    pub conflicting_timestamp: u64,
}

#[event]
#[derive(Debug, Clone)]
pub struct AccessManagerUpdated {
    pub old_access_manager: Pubkey,
    pub new_access_manager: Pubkey,
}

/// Emitted when the attestor set or signature threshold is updated.
#[event]
#[derive(Debug, Clone)]
pub struct AttestorsUpdated {
    pub old_attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
    pub old_min_required_sigs: u8,
    pub new_attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
    pub new_min_required_sigs: u8,
}
