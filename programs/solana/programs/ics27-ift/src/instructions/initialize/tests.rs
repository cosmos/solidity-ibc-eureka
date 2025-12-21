//! Tests for initialize instruction

use anchor_lang::{InstructionData, Space};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    rent::Rent,
};

use crate::state::IFTAppState;
use crate::test_utils::*;

// ============================================================================
// initialize tests
// ============================================================================

/// Create a mock SPL Token mint account with specified decimals
fn create_mock_mint_account(decimals: u8, mint_authority: Pubkey) -> solana_sdk::account::Account {
    // SPL Token Mint layout (82 bytes):
    // - 0..4: mint_authority option (4 bytes: 1 = Some, 0 = None)
    // - 4..36: mint_authority pubkey (32 bytes)
    // - 36..44: supply (8 bytes u64)
    // - 44: decimals (1 byte)
    // - 45: is_initialized (1 byte)
    // - 46..50: freeze_authority option (4 bytes)
    // - 50..82: freeze_authority pubkey (32 bytes)
    let mut data = vec![0u8; 82];

    // mint_authority = Some(mint_authority)
    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    data[4..36].copy_from_slice(&mint_authority.to_bytes());

    // supply = 0
    data[36..44].copy_from_slice(&0u64.to_le_bytes());

    // decimals
    data[44] = decimals;

    // is_initialized = true
    data[45] = 1;

    // freeze_authority = None
    data[46..50].copy_from_slice(&0u32.to_le_bytes());

    solana_sdk::account::Account {
        lamports: 1_000_000,
        data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Test that initialize fails with decimals mismatch
#[test]
fn test_initialize_decimals_mismatch_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let current_mint_authority = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let access_manager = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, _) = get_app_state_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (system_program, system_account) = create_system_program_account();

    // Mint has 6 decimals
    let mint_account = create_mock_mint_account(6, current_mint_authority);

    // App state starts uninitialized
    let app_state_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTAppState::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
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

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    // Pass 9 decimals when mint has 6
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new_readonly(current_mint_authority, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::Initialize {
            decimals: 9, // Wrong! Mint has 6
            access_manager,
            gmp_program,
        }
        .data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (current_mint_authority, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "initialize should fail with decimals mismatch"
    );
}

/// Test that initialize fails when `current_mint_authority` is not a signer
#[test]
fn test_initialize_no_authority_signer_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let current_mint_authority = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let access_manager = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, _) = get_app_state_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (system_program, system_account) = create_system_program_account();

    let mint_account = create_mock_mint_account(6, current_mint_authority);

    let app_state_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTAppState::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
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

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    // current_mint_authority is NOT marked as signer
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new_readonly(current_mint_authority, false), // NOT a signer!
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::Initialize {
            decimals: 6,
            access_manager,
            gmp_program,
        }
        .data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (current_mint_authority, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "initialize should fail when current_mint_authority is not a signer"
    );
}

/// Test that initialize fails with wrong `app_state` PDA seeds
#[test]
fn test_initialize_wrong_pda_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let wrong_mint = Pubkey::new_unique();
    let current_mint_authority = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let access_manager = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    // Wrong PDA - derived from different mint
    let (wrong_app_state_pda, _) = get_app_state_pda(&wrong_mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (system_program, system_account) = create_system_program_account();

    let mint_account = create_mock_mint_account(6, current_mint_authority);

    let app_state_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTAppState::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
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
            AccountMeta::new(wrong_app_state_pda, false), // Wrong PDA!
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new_readonly(current_mint_authority, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::Initialize {
            decimals: 6,
            access_manager,
            gmp_program,
        }
        .data(),
    };

    let accounts = vec![
        (wrong_app_state_pda, app_state_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (current_mint_authority, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "initialize should fail with wrong PDA seeds"
    );
}

/// Test that initialize fails when mint is not owned by token program
#[test]
fn test_initialize_wrong_mint_owner_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let current_mint_authority = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let access_manager = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, _) = get_app_state_pda(&mint);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (system_program, system_account) = create_system_program_account();

    // Mint owned by wrong program
    let mut mint_account = create_mock_mint_account(6, current_mint_authority);
    mint_account.owner = Pubkey::new_unique(); // Wrong owner!

    let app_state_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTAppState::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
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
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new_readonly(current_mint_authority, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::Initialize {
            decimals: 6,
            access_manager,
            gmp_program,
        }
        .data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (current_mint_authority, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "initialize should fail when mint is not owned by token program"
    );
}
