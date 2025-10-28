/// Configuration constants for ICS07 Tendermint Light Client

/// Maximum number of consensus state heights to track in the sorted list
/// This ensures automatic FIFO cleanup when new states are added.
/// With ~1 block per 6 seconds, 10 states = ~1 minute of history.
/// This is sufficient for IBC packet verification while keeping storage minimal.
pub const MAX_CONSENSUS_STATE_HEIGHTS: usize = 10;

