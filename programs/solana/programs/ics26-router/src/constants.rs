//! Common constants used throughout the ICS26 router

/// Size of Anchor instruction discriminator in bytes
pub const ANCHOR_DISCRIMINATOR_SIZE: usize = 8;

/// Initial capacity for IBC CPI instruction data
/// Typical IBC message sizes:
/// - Discriminator: 8 bytes
/// - 2 client IDs: ~32 bytes each
/// - Sequence: 8 bytes
/// - Payload with ports/version/data: ~50-100 bytes
/// - Pubkey: 32 bytes
/// - Total: ~150-200 bytes typical
///   Using 200 to avoid reallocation in most cases
pub const IBC_CPI_INSTRUCTION_CAPACITY: usize = 200;

/// Grace period before packet receipts and acknowledgements can be cleaned up (in seconds)
/// 0 seconds for tests (immediate cleanup), 24 hours for production
#[cfg(feature = "test-grace-period")]
pub const CLEANUP_GRACE_PERIOD: u64 = 0;
#[cfg(not(feature = "test-grace-period"))]
pub const CLEANUP_GRACE_PERIOD: u64 = 86400;

/// Maximum number of receipts/acks that can be cleaned up in a single transaction
pub const MAX_CLEANUP_BATCH_SIZE: u8 = 10;
