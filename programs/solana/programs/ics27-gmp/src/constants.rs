/// Program constants for ICS27 GMP
// Re-export validation constants from solana-ibc-types
pub use solana_ibc_types::{
    MAX_CLIENT_ID_LENGTH, MAX_MEMO_LENGTH, MAX_RECEIVER_LENGTH, MAX_SALT_LENGTH, MAX_SENDER_LENGTH,
};

/// Port ID for this GMP app instance (fixed at compile time)
pub const GMP_PORT_ID: &str = "gmpport";

/// ICS27 version (must match Cosmos GMP module version)
pub const ICS27_VERSION: &str = "ics27-2";

/// ICS27 encoding (must match Cosmos IBC-Go's `EncodingProtobuf` constant)
pub const ICS27_ENCODING: &str = "application/x-protobuf";

/// Maximum timeout duration (24 hours in seconds)
pub const MAX_TIMEOUT_DURATION: i64 = 86400;

/// Minimum timeout duration (12 seconds)
pub const MIN_TIMEOUT_DURATION: i64 = 12;

/// Universal error acknowledgement bytes
pub const ACK_ERROR: &[u8] = b"error";

/// Anchor discriminator size (8 bytes)
pub const DISCRIMINATOR_SIZE: usize = 8;
