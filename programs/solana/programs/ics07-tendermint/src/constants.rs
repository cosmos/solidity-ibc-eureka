/// Configuration constants for ICS07 Tendermint Light Client
/// Default maximum number of consensus states to keep (rolling window)
/// With ~1 block per 6 seconds, this is approximately 2.4 hours of history
pub const DEFAULT_MAX_CONSENSUS_STATES: u16 = 100;

/// Maximum allowed value for `max_consensus_states` to prevent denial of service
pub const MAX_ALLOWED_CONSENSUS_STATES: u16 = 1000;

/// Minimum required consensus states (must keep at least a few for IBC to function)
pub const MIN_REQUIRED_CONSENSUS_STATES: u16 = 10;

/// Grace period before consensus states can be pruned (in seconds)
/// 24 hours = 86400 seconds
pub const CONSENSUS_STATE_PRUNING_GRACE_PERIOD: u64 = 86400;

/// Maximum number of consensus states that can be pruned in a single transaction
pub const MAX_PRUNE_BATCH_SIZE: u8 = 5;

