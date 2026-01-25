use anchor_lang::InstructionData;
use ics26_router::utils::ics24::packet_acknowledgement_commitment_bytes32;
use solana_ibc_types::CallResultStatus;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::evm_selectors::ERROR_ACK_COMMITMENT;
use crate::test_utils::*;

const TEST_CLIENT_ID: &str = "07-tendermint-0";
const TEST_SEQUENCE: u64 = 42;
const TEST_AMOUNT: u64 = 1_000_000;

fn create_token_account(
    mint: &Pubkey,
    owner: &Pubkey,
    amount: u64,
) -> solana_sdk::account::Account {
    let mut data = vec![0u8; 165];
    data[0..32].copy_from_slice(&mint.to_bytes());
    data[32..64].copy_from_slice(&owner.to_bytes());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1; // Initialized

    solana_sdk::account::Account {
        lamports: 1_000_000,
        data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn create_mint_account(mint_authority: Option<&Pubkey>) -> solana_sdk::account::Account {
    let mut data = vec![0u8; 82];
    if let Some(authority) = mint_authority {
        data[0..4].copy_from_slice(&1u32.to_le_bytes()); // Some
        data[4..36].copy_from_slice(&authority.to_bytes());
    }
    data[44] = 9; // decimals
    data[45] = 1; // is_initialized

    solana_sdk::account::Account {
        lamports: 1_000_000,
        data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

struct ClaimRefundTestSetup {
    instruction: Instruction,
    accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
}

fn build_claim_refund_test_setup(
    status: CallResultStatus,
    gmp_result_sender: Pubkey,
    gmp_result_client_id: &str,
    gmp_result_sequence: u64,
) -> ClaimRefundTestSetup {
    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (pending_transfer_pda, pending_transfer_bump) =
        get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
    let (gmp_result_pda, gmp_result_bump) =
        get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE, &gmp_program);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    let pending_transfer_account = create_pending_transfer_account(
        mint,
        TEST_CLIENT_ID,
        TEST_SEQUENCE,
        sender,
        TEST_AMOUNT,
        pending_transfer_bump,
    );

    let gmp_result_account = create_gmp_result_account(
        gmp_result_sender,
        gmp_result_sequence,
        gmp_result_client_id,
        "dest-client",
        status,
        gmp_result_bump,
        &gmp_program,
    );

    let mint_account = create_mint_account(Some(&mint_authority_pda));

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &sender, 0);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new(pending_transfer_pda, false),
            AccountMeta::new_readonly(gmp_result_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::ClaimRefund {
            client_id: TEST_CLIENT_ID.to_string(),
            sequence: TEST_SEQUENCE,
        }
        .data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (pending_transfer_pda, pending_transfer_account),
        (gmp_result_pda, gmp_result_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (sender_token_pda, sender_token_account),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    ClaimRefundTestSetup {
        instruction,
        accounts,
    }
}

#[test]
fn test_error_ack_commitment_matches_runtime_computation() {
    let error_ack = ics26_router::utils::ics24::UNIVERSAL_ERROR_ACK;
    let computed =
        packet_acknowledgement_commitment_bytes32(std::slice::from_ref(&error_ack.to_vec()))
            .expect("single ack is never empty");

    assert_eq!(
        ERROR_ACK_COMMITMENT, computed,
        "Precomputed ERROR_ACK_COMMITMENT must match runtime computation"
    );
}

#[test]
fn test_error_ack_commitment_is_valid() {
    assert_eq!(ERROR_ACK_COMMITMENT.len(), 32);
    assert!(ERROR_ACK_COMMITMENT.iter().any(|&b| b != 0));
}

#[test]
fn test_claim_refund_wrong_gmp_sender_fails() {
    let mollusk = setup_mollusk();

    let wrong_sender = Pubkey::new_unique();
    let setup = build_claim_refund_test_setup(
        CallResultStatus::Timeout,
        wrong_sender,
        TEST_CLIENT_ID,
        TEST_SEQUENCE,
    );

    let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
    assert!(
        result.program_result.is_err(),
        "claim_refund should fail when GMP result sender doesn't match IFT program"
    );
}

#[test]
fn test_claim_refund_wrong_client_id_fails() {
    let mollusk = setup_mollusk();

    let setup = build_claim_refund_test_setup(
        CallResultStatus::Timeout,
        crate::ID,
        "wrong-client-id",
        TEST_SEQUENCE,
    );

    let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
    assert!(
        result.program_result.is_err(),
        "claim_refund should fail when GMP result client ID doesn't match"
    );
}

#[test]
fn test_claim_refund_wrong_sequence_fails() {
    let mollusk = setup_mollusk();

    let setup =
        build_claim_refund_test_setup(CallResultStatus::Timeout, crate::ID, TEST_CLIENT_ID, 999);

    let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
    assert!(
        result.program_result.is_err(),
        "claim_refund should fail when GMP result sequence doesn't match"
    );
}

#[test]
fn test_claim_refund_token_account_wrong_owner_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let wrong_owner = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (pending_transfer_pda, pending_transfer_bump) =
        get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
    let (gmp_result_pda, gmp_result_bump) =
        get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE, &gmp_program);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    let pending_transfer_account = create_pending_transfer_account(
        mint,
        TEST_CLIENT_ID,
        TEST_SEQUENCE,
        sender,
        TEST_AMOUNT,
        pending_transfer_bump,
    );

    let gmp_result_account = create_gmp_result_account(
        crate::ID,
        TEST_SEQUENCE,
        TEST_CLIENT_ID,
        "dest-client",
        CallResultStatus::Timeout,
        gmp_result_bump,
        &gmp_program,
    );

    let mint_account = create_mint_account(Some(&mint_authority_pda));

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &wrong_owner, 0);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new(pending_transfer_pda, false),
            AccountMeta::new_readonly(gmp_result_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::ClaimRefund {
            client_id: TEST_CLIENT_ID.to_string(),
            sequence: TEST_SEQUENCE,
        }
        .data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (pending_transfer_pda, pending_transfer_account),
        (gmp_result_pda, gmp_result_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (sender_token_pda, sender_token_account),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "claim_refund should fail when token account owner doesn't match pending transfer sender"
    );
}

#[test]
fn test_claim_refund_token_account_wrong_mint_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let wrong_mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (pending_transfer_pda, pending_transfer_bump) =
        get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
    let (gmp_result_pda, gmp_result_bump) =
        get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE, &gmp_program);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    let pending_transfer_account = create_pending_transfer_account(
        mint,
        TEST_CLIENT_ID,
        TEST_SEQUENCE,
        sender,
        TEST_AMOUNT,
        pending_transfer_bump,
    );

    let gmp_result_account = create_gmp_result_account(
        crate::ID,
        TEST_SEQUENCE,
        TEST_CLIENT_ID,
        "dest-client",
        CallResultStatus::Timeout,
        gmp_result_bump,
        &gmp_program,
    );

    let mint_account = create_mint_account(Some(&mint_authority_pda));

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&wrong_mint, &sender, 0);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new(pending_transfer_pda, false),
            AccountMeta::new_readonly(gmp_result_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::ClaimRefund {
            client_id: TEST_CLIENT_ID.to_string(),
            sequence: TEST_SEQUENCE,
        }
        .data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (pending_transfer_pda, pending_transfer_account),
        (gmp_result_pda, gmp_result_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (sender_token_pda, sender_token_account),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "claim_refund should fail when token account mint doesn't match"
    );
}
