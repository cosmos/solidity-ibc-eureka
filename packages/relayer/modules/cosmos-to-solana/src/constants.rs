//! Constants for the Cosmos to Solana relayer

/// Anchor account discriminator size (first 8 bytes of account data)
pub const ANCHOR_DISCRIMINATOR_SIZE: usize = 8;

/// GMP (General Message Passing) port identifier
pub const GMP_PORT_ID: &str = "gmpport";

/// Protobuf encoding type for GMP packets
pub const PROTOBUF_ENCODING: &str = "application/x-protobuf";

/// JSON encoding type for IBC packets
pub const JSON_ENCODING: &str = "application/json";

/// GMP account state PDA seed
pub const GMP_ACCOUNT_STATE_SEED: &[u8] = b"gmp_account";
