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

// Anchor custom error codes for `RouterError` variants (offset 6000 + variant index).
pub const PACKET_COMMITMENT_MISMATCH: u32 = 6006;
pub const ASYNC_ACK_NOT_SUPPORTED: u32 = 6009;

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
