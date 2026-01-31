//! Event types for the attestation light client program

use anchor_lang::prelude::*;

/// Event emitted when misbehaviour is detected during client update
#[event]
#[derive(Debug, Clone)]
pub struct MisbehaviourDetected {
    /// The client ID where misbehaviour was detected
    pub client_id: String,
    /// The height at which conflicting timestamps were submitted
    pub height: u64,
    /// The existing timestamp stored for this height
    pub existing_timestamp: u64,
    /// The new conflicting timestamp submitted
    pub conflicting_timestamp: u64,
}
