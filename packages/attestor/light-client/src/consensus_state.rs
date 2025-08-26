//! Minimal attestor consensus state for IBC light client

use serde::{Deserialize, Serialize};

/// Minimal attestor consensus state for IBC light client
/// Contains only the essential information needed for IBC verification:
/// - Height: The attestor height at which this state was created
/// - Timestamp: The timestamp at which this height was reached
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusState {
    /// Attestor height
    pub height: u64,
    /// Timestamp when this height was reached (Unix timestamp in seconds)
    pub timestamp: u64,
}
