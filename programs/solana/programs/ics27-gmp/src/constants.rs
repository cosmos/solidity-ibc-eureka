/// Program constants for ICS27 GMP
///
/// Port ID for this GMP app instance (fixed at compile time)
pub const GMP_PORT_ID: &str = "gmpport";

/// Maximum length for client ID
pub const MAX_CLIENT_ID_LENGTH: usize = 32;

/// Maximum length for sender address (supports both Ethereum hex and Cosmos bech32)
pub const MAX_SENDER_LENGTH: usize = 128; // Supports bech32 addresses up to ~90 chars

/// Maximum length for salt
pub const MAX_SALT_LENGTH: usize = 8;

/// Maximum length for port ID
pub const MAX_PORT_ID_LENGTH: usize = 128;

/// Maximum length for memo
pub const MAX_MEMO_LENGTH: usize = 256;

/// Maximum length for execution payload
pub const MAX_PAYLOAD_LENGTH: usize = 1024;

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
