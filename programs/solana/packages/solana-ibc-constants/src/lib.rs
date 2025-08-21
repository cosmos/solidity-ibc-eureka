//! Constants and program IDs for IBC on Solana
//!
//! This crate provides all the program IDs and constants used by IBC on Solana

/// ICS26 Router Program ID on Solana
pub const ICS26_ROUTER_ID: &str = "FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx";

/// ICS07 Tendermint Light Client Program ID on Solana
pub const ICS07_TENDERMINT_ID: &str = "HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD";

/// Dummy IBC App Program ID (for testing)
pub const DUMMY_IBC_APP_ID: &str = "11111111111111111111111111111111";

/// Mock Light Client Program ID (for testing)
pub const MOCK_LIGHT_CLIENT_ID: &str = "11111111111111111111111111111111";

/// Default IBC version for ICS20
pub const ICS20_VERSION: &str = "ics20-1";

/// Default IBC version for ICS27 (interchain accounts)
pub const ICS27_VERSION: &str = "ics27-1";

/// Default encoding for IBC packets
pub const DEFAULT_ENCODING: &str = "json";

/// Maximum timeout duration (1 day in seconds)
pub const MAX_TIMEOUT_DURATION: i64 = 86400;

/// Default trust level numerator for Tendermint light clients
pub const DEFAULT_TRUST_LEVEL_NUMERATOR: u64 = 2;

/// Default trust level denominator for Tendermint light clients
pub const DEFAULT_TRUST_LEVEL_DENOMINATOR: u64 = 3;

/// Default max clock drift in seconds
pub const DEFAULT_MAX_CLOCK_DRIFT_SECONDS: i64 = 10;
