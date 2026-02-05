//! Program constants for ICS27 IFT

/// Port ID for IFT app instance
pub const IFT_PORT_ID: &str = "iftport";

/// ICS27 version (must match Cosmos IFT module version)
pub const IFT_VERSION: &str = "ift-1";

/// Default timeout duration (15 minutes in seconds)
pub const DEFAULT_TIMEOUT_DURATION: i64 = 60 * 15;

/// Maximum timeout duration (24 hours in seconds)
pub const MAX_TIMEOUT_DURATION: i64 = 60 * 60 * 24;

/// Minimum timeout duration (1 minute in seconds)
pub const MIN_TIMEOUT_DURATION: i64 = 60;

/// Maximum client ID length
pub const MAX_CLIENT_ID_LENGTH: usize = 64;

/// Maximum counterparty address length
pub const MAX_COUNTERPARTY_ADDRESS_LENGTH: usize = 128;

/// Maximum receiver address length
pub const MAX_RECEIVER_LENGTH: usize = 128;

/// PDA seed for IFT app state
pub const IFT_APP_STATE_SEED: &[u8] = b"ift_app_state";

/// PDA seed for IFT bridge
pub const IFT_BRIDGE_SEED: &[u8] = b"ift_bridge";

/// PDA seed for pending transfer
pub const PENDING_TRANSFER_SEED: &[u8] = b"pending_transfer";

/// PDA seed for mint authority
pub const MINT_AUTHORITY_SEED: &[u8] = b"ift_mint_authority";

/// Seconds per day for rate limit day calculation
pub const SECONDS_PER_DAY: u64 = 60 * 60 * 24;
