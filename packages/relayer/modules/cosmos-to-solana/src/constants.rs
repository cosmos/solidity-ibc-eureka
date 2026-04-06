//! Constants for the Cosmos to Solana relayer

/// Anchor account discriminator size (first 8 bytes of account data)
pub const ANCHOR_DISCRIMINATOR_SIZE: usize = 8;

/// GMP (General Message Passing) port identifier
pub const GMP_PORT_ID: &str = "gmpport";

/// Protobuf encoding type for GMP packets
pub const PROTOBUF_ENCODING: &str = "application/x-protobuf";

/// ABI encoding type for GMP packets
pub const ABI_ENCODING: &str = "application/x-solidity-abi";

/// JSON encoding type for IBC packets
pub const JSON_ENCODING: &str = "application/json";

/// Maximum lamports the relayer will pre-fund per GMP packet (~0.05 SOL).
/// Caps the sender-specified `prefund_lamports` to prevent griefing.
pub const MAX_PREFUND_LAMPORTS: u64 = 50_000_000;
