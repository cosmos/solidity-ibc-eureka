use anchor_lang::prelude::*;

/// Emitted when conflicting timestamps are submitted for the same height.
#[event]
#[derive(Debug, Clone)]
pub struct MisbehaviourDetected {
    pub client_id: String,
    pub height: u64,
    pub existing_timestamp: u64,
    pub conflicting_timestamp: u64,
}
