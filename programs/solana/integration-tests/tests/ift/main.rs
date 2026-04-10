//! Solana IFT (Inter-chain Fungible Token) integration tests.
//!
//! A single chain runs as a `ProgramTest` instance with IFT + GMP. The IFT
//! program burns tokens locally and sends a GMP call to the (simulated) EVM
//! counterparty. The relayer delivers ack/timeout back, and `finalize_transfer`
//! either confirms the burn (success ack) or refunds (timeout / error ack).
//!
//! The mock light client always accepts proofs so these tests exercise the full
//! IFT lifecycle without real proof verification.
//!
//! ## Coverage gaps (not testable at integration level)
//!
//! - **Solana-to-Solana roundtrip**: `ChainOptions` only has `Evm` and `Cosmos`
//!   variants; no `Solana` variant exists yet.
//! - **`initialize_existing_token`**: requires an externally-created mint whose
//!   authority signs the transfer — complex setup but doable later.
//! - **Batch relay (single tx with multiple recv/ack)**: tested in e2e only.

use integration_tests::{
    admin::Admin,
    assert_commitment_set, assert_commitment_zeroed,
    chain::{Chain, ChainProgram, TEST_CLOCK_TIME},
    deployer::Deployer,
    ift::{self, IftGmpAckPacketParams, IftGmpTimeoutPacketParams, IftTransferParams, TokenKind},
    ift_admin::IftAdmin,
    programs::{Ics27Gmp, Ift},
    relayer::Relayer,
    user::User,
    Actor,
};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction};

mod admin_transfer;
mod batch_transfers;
mod error_ack_refund;
mod full_lifecycle;
mod pause;
mod timeout_refund;
mod token_2022_lifecycle;

/// IFT timeout: must match `router::test_timeout(TEST_CLOCK_TIME)`.
const IFT_TIMEOUT: u64 = TEST_CLOCK_TIME as u64 + 86_000;

const MINT_DECIMALS: u8 = 6;
const INITIAL_BALANCE: u64 = 1_000_000;
const TRANSFER_AMOUNT: u64 = 100_000;

/// Set up a chain with IFT + GMP, create an SPL token, register a bridge and
/// mint tokens to the user.
///
/// Returns `(mint_pubkey, user_ata)`.
async fn setup_ift_chain(
    chain: &mut Chain,
    ift_admin: &IftAdmin,
    mint_keypair: &Keypair,
    user_pubkey: Pubkey,
) -> (Pubkey, Pubkey) {
    setup_ift_chain_with_token(chain, ift_admin, mint_keypair, user_pubkey, TokenKind::Spl).await
}

/// Set up a chain with IFT + GMP, create a token (SPL or Token 2022), register
/// a bridge and mint tokens to the user.
///
/// Returns `(mint_pubkey, user_ata)`.
async fn setup_ift_chain_with_token(
    chain: &mut Chain,
    ift_admin: &IftAdmin,
    mint_keypair: &Keypair,
    user_pubkey: Pubkey,
    token_kind: TokenKind,
) -> (Pubkey, Pubkey) {
    let mint = mint_keypair.pubkey();
    let admin_pubkey = ift_admin.pubkey();

    // 1. Create token (IFT admin pays + signs as authority)
    let create_token_ix = match token_kind {
        TokenKind::Token2022 => ift::build_create_token_2022_ix(
            admin_pubkey,
            admin_pubkey,
            mint,
            MINT_DECIMALS,
            "Test Token".to_string(),
            "TT".to_string(),
            "https://example.com".to_string(),
        ),
        TokenKind::Spl => {
            ift::build_create_spl_token_ix(admin_pubkey, admin_pubkey, mint, MINT_DECIMALS)
        }
    };
    let tx = Transaction::new_signed_with_payer(
        &[create_token_ix],
        Some(&admin_pubkey),
        &[ift_admin.keypair(), mint_keypair],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("create token");

    // 2. Register EVM bridge (IFT admin pays + signs)
    let register_bridge_ix = ift::build_register_bridge_ix(
        admin_pubkey,
        admin_pubkey,
        mint,
        chain.client_id(),
        ift::COUNTERPARTY_IFT_ADDRESS,
    );
    let tx = Transaction::new_signed_with_payer(
        &[register_bridge_ix],
        Some(&admin_pubkey),
        &[ift_admin.keypair()],
        chain.blockhash(),
    );
    chain
        .process_transaction(tx)
        .await
        .expect("register bridge");

    // 3. Mint tokens to user's ATA (IFT admin pays + signs)
    let admin_mint_ix = ift::build_admin_mint_ix(
        admin_pubkey,
        admin_pubkey,
        mint,
        user_pubkey,
        INITIAL_BALANCE,
        token_kind,
    );
    let tx = Transaction::new_signed_with_payer(
        &[admin_mint_ix],
        Some(&admin_pubkey),
        &[ift_admin.keypair()],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("admin mint");

    let user_ata = token_kind.get_ata(&user_pubkey, &mint);
    (mint, user_ata)
}
