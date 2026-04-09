use ics26_router::errors::RouterError;
use solana_program_test::BanksClientError;
use solana_sdk::{instruction::InstructionError, pubkey::Pubkey, transaction::TransactionError};

pub mod accounts;
pub mod chain;
pub mod gmp;
pub mod relayer;
pub mod router;
pub mod user;

/// Shared interface for test actors (`User`, `Relayer`).
pub trait Actor {
    fn pubkey(&self) -> Pubkey;
}

/// Anchor adds 6000 to the enum discriminant to produce the on-chain error code.
const ANCHOR_ERROR_OFFSET: u32 = 6000;

/// Compute the on-chain Anchor error code from an `#[error_code]` enum variant.
///
/// Works with any Anchor error enum (`RouterError`, `GMPError`, etc.).
/// The formula is `6000 + discriminant` — for enums without explicit
/// discriminants the variant index is used, while enums starting at
/// `= 6000` produce codes like `6000 + 6000 = 12000`.
pub const fn anchor_error_code(variant_discriminant: u32) -> u32 {
    ANCHOR_ERROR_OFFSET.saturating_add(variant_discriminant)
}

pub const PACKET_COMMITMENT_MISMATCH: u32 =
    anchor_error_code(RouterError::PacketCommitmentMismatch as u32);
pub const ASYNC_ACK_NOT_SUPPORTED: u32 =
    anchor_error_code(RouterError::AsyncAcknowledgementNotSupported as u32);

/// Extract the custom error code from a `BanksClientError`, panicking if the
/// error is not an `InstructionError::Custom`.
pub fn extract_custom_error(err: &BanksClientError) -> u32 {
    match err {
        BanksClientError::TransactionError(TransactionError::InstructionError(
            _,
            InstructionError::Custom(code),
        )) => *code,
        other => panic!("expected InstructionError::Custom, got {other:?}"),
    }
}
