//! Integration tests for `pre_verify_signature` instruction.
//!
//! These tests use solana-program-test because the instruction reads from the instructions sysvar
//! to verify a preceding ed25519 program instruction, which requires processing
//! multi-instruction transactions (not possible with mollusk-svm).

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use ed25519_dalek::{Signer, SigningKey};
use ics07_tendermint::state::SignatureVerification;
use sha2::{Digest, Sha256};
use solana_ibc_types::ics07::SignatureData;
use solana_program_test::{ProgramTest, ProgramTestBanksClientExt};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
    signer::Signer as SolSigner, sysvar::instructions as ix_sysvar, transaction::Transaction,
};

const PROGRAM_BINARY_PATH: &str = "../../target/deploy/ics07_tendermint";

/// Creates a `ProgramTest` instance with the `ics07_tendermint` program loaded.
fn setup_program_test() -> ProgramTest {
    // Set SBF_OUT_DIR if not already set, so solana-program-test can find the .so file
    if std::env::var("SBF_OUT_DIR").is_err() {
        let deploy_dir = std::path::Path::new(PROGRAM_BINARY_PATH)
            .parent()
            .expect("Invalid program path");
        std::env::set_var("SBF_OUT_DIR", deploy_dir);
    }

    ProgramTest::new("ics07_tendermint", ics07_tendermint::ID, None)
}

/// Creates a valid `SignatureData` struct from a signing key and message.
fn create_signature_data(signing_key: &SigningKey, msg: &[u8]) -> SignatureData {
    let pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();
    let signature: [u8; 64] = signing_key.sign(msg).to_bytes();
    create_signature_data_raw(pubkey, msg, signature)
}

/// Creates a `SignatureData` struct with explicit pubkey, message, and signature.
/// Useful for testing mismatched values.
fn create_signature_data_raw(pubkey: [u8; 32], msg: &[u8], signature: [u8; 64]) -> SignatureData {
    let mut hasher = Sha256::new();
    hasher.update(pubkey);
    hasher.update(msg);
    hasher.update(signature);
    let signature_hash: [u8; 32] = hasher.finalize().into();

    SignatureData {
        signature_hash,
        pubkey,
        msg: msg.to_vec(),
        signature,
    }
}

/// Ed25519 instruction header length (`num_sigs` + padding + 7x u16 offsets).
const ED25519_HEADER_LEN: u16 = 16;

/// Creates an ed25519 program instruction for signature verification.
/// Must be the first instruction in the transaction for `pre_verify_signature` to work.
fn create_ed25519_instruction(signing_key: &SigningKey, msg: &[u8]) -> Instruction {
    let pubkey = signing_key.verifying_key().to_bytes();
    let signature = signing_key.sign(msg).to_bytes();
    let num_signatures: u8 = 1;
    let padding: u8 = 0;

    // Offsets relative to instruction data start; 0xFFFF = data in same instruction
    let signature_offset: u16 = ED25519_HEADER_LEN;
    let pubkey_offset: u16 = ED25519_HEADER_LEN + 64;
    let message_offset: u16 = ED25519_HEADER_LEN + 64 + 32;
    let same_ix: u16 = 0xFFFF;

    let mut data = Vec::with_capacity((ED25519_HEADER_LEN + 64 + 32) as usize + msg.len());
    data.push(num_signatures);
    data.push(padding);
    data.extend_from_slice(&signature_offset.to_le_bytes());
    data.extend_from_slice(&same_ix.to_le_bytes());
    data.extend_from_slice(&pubkey_offset.to_le_bytes());
    data.extend_from_slice(&same_ix.to_le_bytes());
    data.extend_from_slice(&message_offset.to_le_bytes());
    data.extend_from_slice(&(msg.len() as u16).to_le_bytes());
    data.extend_from_slice(&same_ix.to_le_bytes());
    data.extend_from_slice(&signature);
    data.extend_from_slice(&pubkey);
    data.extend_from_slice(msg);

    Instruction {
        program_id: solana_sdk::ed25519_program::ID,
        accounts: vec![],
        data,
    }
}

/// Creates the `pre_verify_signature` instruction.
fn create_pre_verify_instruction(payer: Pubkey, sig_data: SignatureData) -> (Instruction, Pubkey) {
    let (sig_verification_pda, _bump) = Pubkey::find_program_address(
        &[SignatureVerification::SEED, &sig_data.signature_hash],
        &ics07_tendermint::ID,
    );

    let accounts = ics07_tendermint::accounts::PreVerifySignature {
        instructions_sysvar: ix_sysvar::ID,
        signature_verification: sig_verification_pda,
        payer,
        system_program: solana_sdk::system_program::ID,
    };

    let ix_data = ics07_tendermint::instruction::PreVerifySignature {
        signature: sig_data,
    };

    let instruction = Instruction {
        program_id: ics07_tendermint::ID,
        accounts: accounts.to_account_metas(None),
        data: ix_data.data(),
    };

    (instruction, sig_verification_pda)
}

#[tokio::test]
async fn test_pre_verify_signature_valid() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    // Generate ed25519 keypair and sign a message
    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message for verification";
    let sig_data = create_signature_data(&signing_key, msg);

    // Create instructions
    let ed25519_ix = create_ed25519_instruction(&signing_key, msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    // Build transaction with BOTH instructions (ed25519 must be first)
    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // Execute
    banks_client.process_transaction(tx).await.unwrap();

    // Verify the SignatureVerification account was created with is_valid = true
    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    // Skip the 8-byte Anchor discriminator
    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(verification.is_valid, "Signature should be valid");
    assert_eq!(
        verification.submitter,
        payer.pubkey(),
        "Submitter should match payer"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_wrong_pubkey_returns_invalid() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    // Sign with one key
    let real_signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";
    let signature = real_signing_key.sign(msg).to_bytes();

    // But claim it was signed by a different key
    let wrong_key = SigningKey::generate(&mut rand::thread_rng());
    let sig_data = create_signature_data_raw(wrong_key.verifying_key().to_bytes(), msg, signature);

    // Ed25519 instruction with CORRECT key (runtime will verify this)
    let ed25519_ix = create_ed25519_instruction(&real_signing_key, msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client.process_transaction(tx).await.unwrap();

    // Signature should be marked as INVALID due to pubkey mismatch
    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(
        !verification.is_valid,
        "Signature should be invalid due to pubkey mismatch"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_wrong_message_returns_invalid() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let real_msg = b"real message";
    let fake_msg = b"fake message";

    // Sign the real message, but claim it was for fake message
    let signature = signing_key.sign(real_msg).to_bytes();
    let tampered_sig_data =
        create_signature_data_raw(signing_key.verifying_key().to_bytes(), fake_msg, signature);

    // Ed25519 instruction verifies with real message
    let ed25519_ix = create_ed25519_instruction(&signing_key, real_msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), tampered_sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client.process_transaction(tx).await.unwrap();

    // Signature should be marked as INVALID due to message mismatch
    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(
        !verification.is_valid,
        "Signature should be invalid due to message mismatch"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_wrong_signature_returns_invalid() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";

    // Create a different signature (sign a different message) but claim it's for msg
    let different_sig = signing_key.sign(b"different message").to_bytes();
    let tampered_sig_data =
        create_signature_data_raw(signing_key.verifying_key().to_bytes(), msg, different_sig);

    // Ed25519 instruction with correct signature
    let ed25519_ix = create_ed25519_instruction(&signing_key, msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), tampered_sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client.process_transaction(tx).await.unwrap();

    // Signature should be marked as INVALID due to signature mismatch
    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(
        !verification.is_valid,
        "Signature should be invalid due to signature mismatch"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_no_ed25519_instruction_returns_invalid() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let sig_data = create_signature_data(&signing_key, b"test message");

    // Only pre_verify instruction - NO ed25519 instruction
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[pre_verify_ix], // Only one instruction - missing ed25519
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // Transaction succeeds but verification should be invalid
    // (instruction at index 0 is our own instruction, not ed25519)
    banks_client.process_transaction(tx).await.unwrap();

    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(
        !verification.is_valid,
        "Signature should be invalid when no ed25519 instruction present"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_multiple_signatures_not_supported() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";
    let sig_data = create_signature_data(&signing_key, msg);

    // Create ed25519 instruction claiming 2 signatures
    let pubkey = signing_key.verifying_key().to_bytes();
    let signature = signing_key.sign(msg).to_bytes();

    let num_signatures: u8 = 2; // WRONG - we only support 1
    let mut data = Vec::with_capacity(16 + 64 + 32 + msg.len());
    data.push(num_signatures);
    data.push(0); // padding
    data.extend_from_slice(&16u16.to_le_bytes()); // signature offset
    data.extend_from_slice(&0xFFFFu16.to_le_bytes());
    data.extend_from_slice(&80u16.to_le_bytes()); // pubkey offset
    data.extend_from_slice(&0xFFFFu16.to_le_bytes());
    data.extend_from_slice(&112u16.to_le_bytes()); // message offset
    data.extend_from_slice(&(msg.len() as u16).to_le_bytes());
    data.extend_from_slice(&0xFFFFu16.to_le_bytes());
    data.extend_from_slice(&signature);
    data.extend_from_slice(&pubkey);
    data.extend_from_slice(msg);

    let ed25519_ix = Instruction {
        program_id: solana_sdk::ed25519_program::ID,
        accounts: vec![],
        data,
    };

    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // Transaction might fail at ed25519 precompile level or succeed with invalid
    let result = banks_client.process_transaction(tx).await;

    if result.is_ok() {
        let account = banks_client
            .get_account(sig_verification_pda)
            .await
            .unwrap()
            .expect("Account not found");

        let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
        assert!(
            !verification.is_valid,
            "Should be invalid for multiple signatures"
        );
    }
    // If it fails, that's also acceptable
}

#[tokio::test]
async fn test_pre_verify_signature_empty_message() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b""; // Empty message
    let sig_data = create_signature_data(&signing_key, msg);

    let ed25519_ix = create_ed25519_instruction(&signing_key, msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client
        .process_transaction(tx)
        .await
        .expect("Empty message should be valid");

    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(
        verification.is_valid,
        "Empty message signature should be valid"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_large_message() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    // Large message (but not too large to fit in transaction)
    let msg: Vec<u8> = (0u8..=255).cycle().take(500).collect();
    let sig_data = create_signature_data(&signing_key, &msg);

    let ed25519_ix = create_ed25519_instruction(&signing_key, &msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client
        .process_transaction(tx)
        .await
        .expect("Large message should be valid");

    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(
        verification.is_valid,
        "Large message signature should be valid"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_ed25519_at_wrong_index_returns_invalid() {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";
    let sig_data = create_signature_data(&signing_key, msg);

    // Create a dummy instruction to put at index 0 (compute budget is a no-op that always succeeds)
    let dummy_ix = ComputeBudgetInstruction::set_compute_unit_limit(200_000);

    let ed25519_ix = create_ed25519_instruction(&signing_key, msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    // Put ed25519 at index 1 instead of index 0
    let tx = Transaction::new_signed_with_payer(
        &[dummy_ix, ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client.process_transaction(tx).await.unwrap();

    // Signature should be invalid because ed25519 is not at index 0
    let account = banks_client
        .get_account(sig_verification_pda)
        .await
        .unwrap()
        .expect("Account not found");

    let verification = SignatureVerification::deserialize(&mut &account.data[8..]).unwrap();
    assert!(
        !verification.is_valid,
        "Signature should be invalid when ed25519 instruction is not at index 0"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_duplicate_pda_fails() {
    let pt = setup_program_test();
    let (mut banks_client, payer, recent_blockhash) = pt.start().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";
    let sig_data = create_signature_data(&signing_key, msg);

    let ed25519_ix = create_ed25519_instruction(&signing_key, msg);
    let (pre_verify_ix, _sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data.clone());

    // First transaction - should succeed
    let tx1 = Transaction::new_signed_with_payer(
        &[ed25519_ix.clone(), pre_verify_ix.clone()],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(tx1).await.unwrap();

    // Second transaction with same signature - should fail (PDA already exists)
    let new_blockhash = banks_client
        .get_new_latest_blockhash(&recent_blockhash)
        .await
        .unwrap();

    let (pre_verify_ix2, _) = create_pre_verify_instruction(payer.pubkey(), sig_data);
    let tx2 = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix2],
        Some(&payer.pubkey()),
        &[&payer],
        new_blockhash,
    );

    let result = banks_client.process_transaction(tx2).await;
    assert!(
        result.is_err(),
        "Second verification with same signature should fail"
    );
}
