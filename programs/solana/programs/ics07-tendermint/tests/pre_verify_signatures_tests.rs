//! Integration tests for pre_verify_signature instruction.
//!
//! These tests use solana-program-test because the instruction reads from the instructions sysvar
//! to verify a preceding ed25519 program instruction, which requires processing
//! multi-instruction transactions (not possible with mollusk-svm).

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use ed25519_dalek::{Signer, SigningKey};
use ics07_tendermint::state::SignatureVerification;
use sha2::{Digest, Sha256};
use solana_ibc_types::ics07::SignatureData;
use solana_program_test::ProgramTest;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signer::Signer as SolSigner,
    system_program,
    sysvar::instructions as ix_sysvar,
    transaction::Transaction,
};

/// Creates a ProgramTest instance with the ics07_tendermint program loaded.
fn setup_program_test() -> ProgramTest {
    // Load the program from .so file (None uses BPF loader)
    ProgramTest::new(
        "ics07_tendermint",
        ics07_tendermint::ID,
        None, // Load from target/deploy/ics07_tendermint.so
    )
}

/// Creates a valid SignatureData struct from a signing key and message.
fn create_signature_data(signing_key: &SigningKey, msg: &[u8]) -> SignatureData {
    let signature = signing_key.sign(msg);
    let pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();
    let sig_bytes: [u8; 64] = signature.to_bytes();

    // Compute signature hash (used for PDA derivation)
    let mut hasher = Sha256::new();
    hasher.update(pubkey);
    hasher.update(msg);
    hasher.update(sig_bytes);
    let signature_hash: [u8; 32] = hasher.finalize().into();

    SignatureData {
        signature_hash,
        pubkey,
        msg: msg.to_vec(),
        signature: sig_bytes,
    }
}

/// Creates an ed25519 program instruction for signature verification.
/// This must be the first instruction in the transaction for pre_verify_signature to work.
fn create_ed25519_instruction(signing_key: &SigningKey, msg: &[u8]) -> Instruction {
    let pubkey = signing_key.verifying_key().to_bytes();
    let signature = signing_key.sign(msg).to_bytes();

    // Ed25519 instruction format:
    // - 1 byte: number of signatures (1)
    // - 1 byte: padding
    // - 2 bytes: signature offset
    // - 2 bytes: signature instruction index (0xFFFF = same instruction)
    // - 2 bytes: public key offset
    // - 2 bytes: public key instruction index
    // - 2 bytes: message data offset
    // - 2 bytes: message data size
    // - 2 bytes: message instruction index
    // - signature bytes (64)
    // - public key bytes (32)
    // - message bytes

    let num_signatures: u8 = 1;
    let padding: u8 = 0;

    // Offsets are relative to the start of instruction data
    let signature_offset: u16 = 16; // After header
    let signature_ix_index: u16 = 0xFFFF; // Same instruction
    let pubkey_offset: u16 = 16 + 64; // After signature
    let pubkey_ix_index: u16 = 0xFFFF;
    let message_offset: u16 = 16 + 64 + 32; // After pubkey
    let message_size: u16 = msg.len() as u16;
    let message_ix_index: u16 = 0xFFFF;

    let mut data = Vec::with_capacity(16 + 64 + 32 + msg.len());
    data.push(num_signatures);
    data.push(padding);
    data.extend_from_slice(&signature_offset.to_le_bytes());
    data.extend_from_slice(&signature_ix_index.to_le_bytes());
    data.extend_from_slice(&pubkey_offset.to_le_bytes());
    data.extend_from_slice(&pubkey_ix_index.to_le_bytes());
    data.extend_from_slice(&message_offset.to_le_bytes());
    data.extend_from_slice(&message_size.to_le_bytes());
    data.extend_from_slice(&message_ix_index.to_le_bytes());
    data.extend_from_slice(&signature);
    data.extend_from_slice(&pubkey);
    data.extend_from_slice(msg);

    Instruction {
        program_id: solana_sdk::ed25519_program::ID,
        accounts: vec![],
        data,
    }
}

/// Creates the pre_verify_signature instruction.
fn create_pre_verify_instruction(payer: Pubkey, sig_data: SignatureData) -> (Instruction, Pubkey) {
    let (sig_verification_pda, _bump) = Pubkey::find_program_address(
        &[SignatureVerification::SEED, &sig_data.signature_hash],
        &ics07_tendermint::ID,
    );

    let accounts = ics07_tendermint::accounts::PreVerifySignature {
        instructions_sysvar: ix_sysvar::ID,
        signature_verification: sig_verification_pda,
        payer,
        system_program: system_program::ID,
    };

    let ix_data = ics07_tendermint::instruction::PreVerifySignature { signature: sig_data };

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
    let signature = real_signing_key.sign(msg);

    // But claim it was signed by a different key
    let wrong_key = SigningKey::generate(&mut rand::thread_rng());

    // Create SignatureData with WRONG pubkey
    let mut hasher = Sha256::new();
    hasher.update(wrong_key.verifying_key().to_bytes());
    hasher.update(msg);
    hasher.update(signature.to_bytes());
    let signature_hash: [u8; 32] = hasher.finalize().into();

    let sig_data = SignatureData {
        signature_hash,
        pubkey: wrong_key.verifying_key().to_bytes(), // WRONG pubkey
        msg: msg.to_vec(),
        signature: signature.to_bytes(),
    };

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

    // Sign the real message
    let sig_data = create_signature_data(&signing_key, real_msg);

    // Create SignatureData claiming it was for fake message
    let mut hasher = Sha256::new();
    hasher.update(signing_key.verifying_key().to_bytes());
    hasher.update(fake_msg);
    hasher.update(sig_data.signature);
    let signature_hash: [u8; 32] = hasher.finalize().into();

    let tampered_sig_data = SignatureData {
        signature_hash,
        pubkey: sig_data.pubkey,
        msg: fake_msg.to_vec(), // WRONG message
        signature: sig_data.signature,
    };

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

    // Create valid signature data
    let sig_data = create_signature_data(&signing_key, msg);

    // Create a different signature (sign a different message)
    let different_sig = signing_key.sign(b"different message").to_bytes();

    // Create SignatureData with WRONG signature bytes
    let mut hasher = Sha256::new();
    hasher.update(signing_key.verifying_key().to_bytes());
    hasher.update(msg);
    hasher.update(different_sig);
    let signature_hash: [u8; 32] = hasher.finalize().into();

    let tampered_sig_data = SignatureData {
        signature_hash,
        pubkey: sig_data.pubkey,
        msg: msg.to_vec(),
        signature: different_sig, // WRONG signature
    };

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
    let sig_data = create_signature_data(&signing_key, b"msg");

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
    let msg: &[u8] = b""; // Empty message
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
    let msg: Vec<u8> = (0..500).map(|i| (i % 256) as u8).collect();
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
