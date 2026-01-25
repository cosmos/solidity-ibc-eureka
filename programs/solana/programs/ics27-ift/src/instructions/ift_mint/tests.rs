//! Tests for `ift_mint` instruction

use anchor_lang::InstructionData;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::state::{CounterpartyChainType, IFTMintMsg};
use crate::test_utils::*;

const TEST_CLIENT_ID: &str = "07-tendermint-0";
const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";

#[test]
fn test_ift_mint_zero_amount_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let receiver = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (gmp_account_pda, gmp_account_bump) =
        get_gmp_account_pda(TEST_CLIENT_ID, TEST_COUNTERPARTY_ADDRESS, &gmp_program);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    let ift_bridge_account = create_ift_bridge_account(
        mint,
        TEST_CLIENT_ID,
        TEST_COUNTERPARTY_ADDRESS,
        "", // denom not used for EVM
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: vec![0; 82],
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let receiver_token_pda = Pubkey::new_unique();
    let mut receiver_token_data = vec![0u8; 165];
    receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
    receiver_token_data[32..64].copy_from_slice(&receiver.to_bytes());
    let receiver_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: receiver_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let associated_token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    // Zero amount!
    let msg = IFTMintMsg {
        receiver,
        amount: 0,
        client_id: TEST_CLIENT_ID.to_string(),
        gmp_account_bump,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(receiver_token_pda, false),
            AccountMeta::new_readonly(receiver, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new_readonly(gmp_account_pda, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::IftMint { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (receiver_token_pda, receiver_token_account),
        (receiver, create_signer_account()),
        (gmp_program, create_gmp_program_account()),
        (gmp_account_pda, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (
            anchor_spl::associated_token::ID,
            associated_token_program_account,
        ),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_mint should fail with zero amount"
    );
}

#[test]
fn test_ift_mint_receiver_mismatch_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let receiver = Pubkey::new_unique();
    let wrong_receiver = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (gmp_account_pda, gmp_account_bump) =
        get_gmp_account_pda(TEST_CLIENT_ID, TEST_COUNTERPARTY_ADDRESS, &gmp_program);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    let ift_bridge_account = create_ift_bridge_account(
        mint,
        TEST_CLIENT_ID,
        TEST_COUNTERPARTY_ADDRESS,
        "", // denom not used for EVM
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: vec![0; 82],
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    // Token account for wrong_receiver
    let receiver_token_pda = Pubkey::new_unique();
    let mut receiver_token_data = vec![0u8; 165];
    receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
    receiver_token_data[32..64].copy_from_slice(&wrong_receiver.to_bytes());
    let receiver_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: receiver_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let associated_token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    // Message receiver doesn't match receiver_owner account
    let msg = IFTMintMsg {
        receiver, // expects 'receiver'
        amount: 1000,
        client_id: TEST_CLIENT_ID.to_string(),
        gmp_account_bump,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(receiver_token_pda, false),
            AccountMeta::new_readonly(wrong_receiver, false), // wrong receiver!
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new_readonly(gmp_account_pda, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::IftMint { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (receiver_token_pda, receiver_token_account),
        (wrong_receiver, create_signer_account()),
        (gmp_program, create_gmp_program_account()),
        (gmp_account_pda, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (
            anchor_spl::associated_token::ID,
            associated_token_program_account,
        ),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_mint should fail with receiver mismatch"
    );
}

#[test]
fn test_ift_mint_gmp_not_signer_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let receiver = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (gmp_account_pda, gmp_account_bump) =
        get_gmp_account_pda(TEST_CLIENT_ID, TEST_COUNTERPARTY_ADDRESS, &gmp_program);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    let ift_bridge_account = create_ift_bridge_account(
        mint,
        TEST_CLIENT_ID,
        TEST_COUNTERPARTY_ADDRESS,
        "", // denom not used for EVM
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: vec![0; 82],
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let receiver_token_pda = Pubkey::new_unique();
    let mut receiver_token_data = vec![0u8; 165];
    receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
    receiver_token_data[32..64].copy_from_slice(&receiver.to_bytes());
    let receiver_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: receiver_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let associated_token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let msg = IFTMintMsg {
        receiver,
        amount: 1000,
        client_id: TEST_CLIENT_ID.to_string(),
        gmp_account_bump,
    };

    // GMP account is NOT marked as signer
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(receiver_token_pda, false),
            AccountMeta::new_readonly(receiver, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new_readonly(gmp_account_pda, false), // NOT a signer!
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::IftMint { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (receiver_token_pda, receiver_token_account),
        (receiver, create_signer_account()),
        (gmp_program, create_gmp_program_account()),
        (gmp_account_pda, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (
            anchor_spl::associated_token::ID,
            associated_token_program_account,
        ),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_mint should fail when GMP account is not a signer"
    );
}

#[test]
fn test_ift_mint_bridge_not_active_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let receiver = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (gmp_account_pda, gmp_account_bump) =
        get_gmp_account_pda(TEST_CLIENT_ID, TEST_COUNTERPARTY_ADDRESS, &gmp_program);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    // Bridge is NOT active!
    let ift_bridge_account = create_ift_bridge_account(
        mint,
        TEST_CLIENT_ID,
        TEST_COUNTERPARTY_ADDRESS,
        "", // denom not used for EVM
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        false, // not active
    );

    let mint_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: vec![0; 82],
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let receiver_token_pda = Pubkey::new_unique();
    let mut receiver_token_data = vec![0u8; 165];
    receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
    receiver_token_data[32..64].copy_from_slice(&receiver.to_bytes());
    let receiver_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: receiver_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let associated_token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let msg = IFTMintMsg {
        receiver,
        amount: 1000,
        client_id: TEST_CLIENT_ID.to_string(),
        gmp_account_bump,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(receiver_token_pda, false),
            AccountMeta::new_readonly(receiver, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new_readonly(gmp_account_pda, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::IftMint { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (receiver_token_pda, receiver_token_account),
        (receiver, create_signer_account()),
        (gmp_program, create_gmp_program_account()),
        (gmp_account_pda, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (
            anchor_spl::associated_token::ID,
            associated_token_program_account,
        ),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_mint should fail when bridge is not active"
    );
}

#[test]
fn test_ift_mint_invalid_gmp_account_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let receiver = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    // Use a WRONG GMP account (random pubkey instead of correct PDA)
    let wrong_gmp_account = Pubkey::new_unique();
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        gmp_program,
    );

    let ift_bridge_account = create_ift_bridge_account(
        mint,
        TEST_CLIENT_ID,
        TEST_COUNTERPARTY_ADDRESS,
        "", // denom not used for EVM
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: vec![0; 82],
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let mint_authority_account = solana_sdk::account::Account {
        lamports: 0,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let receiver_token_pda = Pubkey::new_unique();
    let mut receiver_token_data = vec![0u8; 165];
    receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
    receiver_token_data[32..64].copy_from_slice(&receiver.to_bytes());
    let receiver_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: receiver_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let associated_token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    // Use a dummy bump since the GMP account is wrong anyway
    let msg = IFTMintMsg {
        receiver,
        amount: 1000,
        client_id: TEST_CLIENT_ID.to_string(),
        gmp_account_bump: 255,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(receiver_token_pda, false),
            AccountMeta::new_readonly(receiver, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new_readonly(wrong_gmp_account, true), // Wrong GMP account!
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::IftMint { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (receiver_token_pda, receiver_token_account),
        (receiver, create_signer_account()),
        (gmp_program, create_gmp_program_account()),
        (wrong_gmp_account, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (
            anchor_spl::associated_token::ID,
            associated_token_program_account,
        ),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_mint should fail with invalid GMP account"
    );
}
