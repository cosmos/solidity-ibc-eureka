use ics26_router::errors::RouterError;
use solana_program_test::BanksClientError;
use solana_sdk::{instruction::InstructionError, pubkey::Pubkey, transaction::TransactionError};

pub mod accounts;
pub mod actors;
pub mod chain;
pub mod gmp;
pub mod ift;
pub mod programs;
pub mod router;

pub use actors::{admin, deployer, ift_admin, relayer, user, Actor};

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

/// Assert a commitment PDA has non-zero data (exists after send).
pub async fn assert_commitment_set(chain: &chain::Chain, pda: Pubkey) {
    let account = chain
        .get_account(pda)
        .await
        .expect("commitment should exist");
    assert_ne!(
        &account.data[8..40],
        &[0u8; 32],
        "commitment should be non-zero"
    );
}

/// Assert a commitment PDA has zeroed data (consumed by ack/timeout).
pub async fn assert_commitment_zeroed(chain: &chain::Chain, pda: Pubkey) {
    let account = chain
        .get_account(pda)
        .await
        .expect("commitment should exist");
    assert_eq!(
        &account.data[8..40],
        &[0u8; 32],
        "commitment should be zeroed"
    );
}

/// Assert a receipt PDA exists and is owned by the router.
pub async fn assert_receipt_created(chain: &chain::Chain, pda: Pubkey) {
    let account = chain.get_account(pda).await.expect("receipt should exist");
    assert_eq!(account.owner, ics26_router::ID);
}

/// Read ack commitment bytes from an ack PDA.
pub async fn extract_ack_data(chain: &chain::Chain, ack_pda: Pubkey) -> Vec<u8> {
    chain
        .get_account(ack_pda)
        .await
        .expect("ack should exist")
        .data[8..40]
        .to_vec()
}
