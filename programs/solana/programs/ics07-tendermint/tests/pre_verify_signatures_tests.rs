use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use ed25519_dalek::{Signer, SigningKey};
use ics07_tendermint::state::SignatureVerification;
use rstest::{fixture, rstest};
use sha2::{Digest, Sha256};
use solana_ibc_types::ics07::SignatureData;
use solana_program_test::{BanksClient, ProgramTest, ProgramTestBanksClientExt};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, hash::Hash, instruction::Instruction, pubkey::Pubkey,
    signature::Keypair, signer::Signer as SolSigner, sysvar::instructions as ix_sysvar,
    transaction::Transaction,
};

const PROGRAM_BINARY_PATH: &str = "../../target/deploy/ics07_tendermint";

/// Creates a `ProgramTest` instance with the `ics07_tendermint` program loaded.
fn setup_program_test() -> ProgramTest {
    if std::env::var("SBF_OUT_DIR").is_err() {
        let deploy_dir = std::path::Path::new(PROGRAM_BINARY_PATH)
            .parent()
            .expect("Invalid program path");
        std::env::set_var("SBF_OUT_DIR", deploy_dir);
    }
    ProgramTest::new("ics07_tendermint", ics07_tendermint::ID, None)
}

struct TestContext {
    banks_client: BanksClient,
    payer: Keypair,
    recent_blockhash: Hash,
}

#[fixture]
async fn ctx() -> TestContext {
    let pt = setup_program_test();
    let (banks_client, payer, recent_blockhash) = pt.start().await;
    TestContext {
        banks_client,
        payer,
        recent_blockhash,
    }
}

/// Creates a valid `SignatureData` struct from a signing key and message.
fn create_signature_data(signing_key: &SigningKey, msg: &[u8]) -> SignatureData {
    let pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();
    let signature: [u8; 64] = signing_key.sign(msg).to_bytes();
    create_signature_data_raw(pubkey, msg, signature)
}

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

    let signature_offset: u16 = ED25519_HEADER_LEN;
    let pubkey_offset: u16 = ED25519_HEADER_LEN + 64;
    let message_offset: u16 = ED25519_HEADER_LEN + 64 + 32;
    let same_ix: u16 = 0xFFFF;

    let mut data = Vec::with_capacity((ED25519_HEADER_LEN + 64 + 32) as usize + msg.len());
    data.push(1u8); // num_signatures
    data.push(0); // padding
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

async fn get_verification(banks_client: &BanksClient, pda: Pubkey) -> SignatureVerification {
    let account = banks_client
        .get_account(pda)
        .await
        .unwrap()
        .expect("Account not found");
    // Skip the 8-byte Anchor discriminator
    SignatureVerification::deserialize(&mut &account.data[8..]).unwrap()
}

#[rstest]
#[case::normal_message(b"test message for verification".to_vec())]
#[case::empty_message(vec![])]
#[case::large_message((0u8..=255).cycle().take(500).collect())]
#[tokio::test]
async fn test_pre_verify_signature_valid(#[future] ctx: TestContext, #[case] msg: Vec<u8>) {
    let TestContext {
        banks_client,
        payer,
        recent_blockhash,
    } = ctx.await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
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

    banks_client.process_transaction(tx).await.unwrap();

    let verification = get_verification(&banks_client, sig_verification_pda).await;
    assert!(verification.is_valid, "Signature should be valid");
    assert_eq!(verification.submitter, payer.pubkey());
}

enum TamperType {
    Pubkey,
    Message,
    Signature,
}

fn create_tampered_sig_data(
    signing_key: &SigningKey,
    tamper: TamperType,
) -> (SignatureData, Vec<u8>) {
    let msg = b"test message";
    let real_msg = msg.to_vec();

    match tamper {
        TamperType::Pubkey => {
            let signature = signing_key.sign(msg).to_bytes();
            let wrong_key = SigningKey::generate(&mut rand::thread_rng());
            let sig_data =
                create_signature_data_raw(wrong_key.verifying_key().to_bytes(), msg, signature);
            (sig_data, real_msg)
        }
        TamperType::Message => {
            let signature = signing_key.sign(msg).to_bytes();
            let sig_data = create_signature_data_raw(
                signing_key.verifying_key().to_bytes(),
                b"fake message",
                signature,
            );
            (sig_data, real_msg)
        }
        TamperType::Signature => {
            let different_sig = signing_key.sign(b"different message").to_bytes();
            let sig_data = create_signature_data_raw(
                signing_key.verifying_key().to_bytes(),
                msg,
                different_sig,
            );
            (sig_data, real_msg)
        }
    }
}

#[rstest]
#[case::wrong_pubkey(TamperType::Pubkey)]
#[case::wrong_message(TamperType::Message)]
#[case::wrong_signature(TamperType::Signature)]
#[tokio::test]
async fn test_pre_verify_signature_tampered_returns_invalid(
    #[future] ctx: TestContext,
    #[case] tamper: TamperType,
) {
    let TestContext {
        banks_client,
        payer,
        recent_blockhash,
    } = ctx.await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let (sig_data, real_msg) = create_tampered_sig_data(&signing_key, tamper);

    let ed25519_ix = create_ed25519_instruction(&signing_key, &real_msg);
    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client.process_transaction(tx).await.unwrap();

    let verification = get_verification(&banks_client, sig_verification_pda).await;
    assert!(
        !verification.is_valid,
        "Signature should be invalid due to tampered data"
    );
}

#[rstest]
#[case::no_ed25519_instruction(false)]
#[case::ed25519_at_wrong_index(true)]
#[tokio::test]
async fn test_pre_verify_signature_ed25519_position_invalid(
    #[future] ctx: TestContext,
    #[case] include_ed25519: bool,
) {
    let TestContext {
        banks_client,
        payer,
        recent_blockhash,
    } = ctx.await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";
    let sig_data = create_signature_data(&signing_key, msg);

    let (pre_verify_ix, sig_verification_pda) =
        create_pre_verify_instruction(payer.pubkey(), sig_data);

    let instructions = if include_ed25519 {
        let dummy_ix = ComputeBudgetInstruction::set_compute_unit_limit(200_000);
        let ed25519_ix = create_ed25519_instruction(&signing_key, msg);
        vec![dummy_ix, ed25519_ix, pre_verify_ix]
    } else {
        vec![pre_verify_ix]
    };

    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client.process_transaction(tx).await.unwrap();

    let verification = get_verification(&banks_client, sig_verification_pda).await;
    assert!(
        !verification.is_valid,
        "Signature should be invalid due to ed25519 position"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_malformed_ed25519_fails() {
    let TestContext {
        banks_client,
        payer,
        recent_blockhash,
    } = ctx().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";
    let sig_data = create_signature_data(&signing_key, msg);

    let pubkey = signing_key.verifying_key().to_bytes();
    let signature = signing_key.sign(msg).to_bytes();

    let mut data = Vec::with_capacity(16 + 64 + 32 + msg.len());
    data.push(2u8); // num_signatures = 2 (but we only provide 1)
    data.push(0);
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(&0xFFFFu16.to_le_bytes());
    data.extend_from_slice(&80u16.to_le_bytes());
    data.extend_from_slice(&0xFFFFu16.to_le_bytes());
    data.extend_from_slice(&112u16.to_le_bytes());
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

    let (pre_verify_ix, _) = create_pre_verify_instruction(payer.pubkey(), sig_data);

    let tx = Transaction::new_signed_with_payer(
        &[ed25519_ix, pre_verify_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    assert!(
        banks_client.process_transaction(tx).await.is_err(),
        "Malformed ed25519 instruction should fail at precompile"
    );
}

#[tokio::test]
async fn test_pre_verify_signature_duplicate_pda_fails() {
    let TestContext {
        mut banks_client,
        payer,
        recent_blockhash,
    } = ctx().await;

    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let msg = b"test message";
    let sig_data = create_signature_data(&signing_key, msg);

    let ed25519_ix = create_ed25519_instruction(&signing_key, msg);
    let (pre_verify_ix, _) = create_pre_verify_instruction(payer.pubkey(), sig_data.clone());

    let tx1 = Transaction::new_signed_with_payer(
        &[ed25519_ix.clone(), pre_verify_ix.clone()],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(tx1).await.unwrap();

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

    assert!(
        banks_client.process_transaction(tx2).await.is_err(),
        "Second verification with same signature should fail"
    );
}
