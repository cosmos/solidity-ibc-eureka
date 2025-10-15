//! Constants for the Cosmos to Solana relayer

/// GMP (General Message Passing) port identifier
pub const GMP_PORT_ID: &str = "gmpport";

/// Protobuf encoding type for GMP packets
pub const PROTOBUF_ENCODING: &str = "application/x-protobuf";

/// GMP account state PDA seed
pub const GMP_ACCOUNT_STATE_SEED: &[u8] = b"gmp_account";
