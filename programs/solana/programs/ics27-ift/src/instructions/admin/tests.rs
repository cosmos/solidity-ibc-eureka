//! Tests for admin instructions (pause, unpause, `set_access_manager`)

use anchor_lang::InstructionData;
use mollusk_svm::Mollusk;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::test_utils::*;

fn setup_test_mollusk() -> Mollusk {
    setup_mollusk()
}

// ============================================================================
// pause_app tests
// ============================================================================

#[test]
fn test_pause_app_success() {
    let mollusk = setup_test_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_pauser(admin, admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(), // gmp_program
        false,                // not paused
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(instructions_sysvar, false),
        ],
        data: crate::instruction::PauseApp {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (access_manager_pda, access_manager_account),
        (admin, create_signer_account()),
        (instructions_sysvar, instructions_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        !result.program_result.is_err(),
        "pause_app should succeed: {:?}",
        result.program_result
    );

    // Verify app state is now paused
    let updated_account = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == app_state_pda)
        .expect("app state should exist")
        .1
        .clone();
    let updated_state = deserialize_app_state(&updated_account);
    assert!(updated_state.paused, "App should be paused");
}

#[test]
fn test_pause_app_already_paused_fails() {
    let mollusk = setup_test_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_pauser(admin, admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

    // Already paused
    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
        true, // already paused
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(instructions_sysvar, false),
        ],
        data: crate::instruction::PauseApp {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (access_manager_pda, access_manager_account),
        (admin, create_signer_account()),
        (instructions_sysvar, instructions_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "pause_app should fail when already paused"
    );
}

#[test]
fn test_pause_app_unauthorized_fails() {
    let mollusk = setup_test_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let unauthorized = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    // Access manager only has admin, not the unauthorized user as pauser
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_admin(admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
        false,
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(unauthorized, true), // Not a pauser
            AccountMeta::new_readonly(instructions_sysvar, false),
        ],
        data: crate::instruction::PauseApp {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (access_manager_pda, access_manager_account),
        (unauthorized, create_signer_account()),
        (instructions_sysvar, instructions_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "pause_app should fail for unauthorized user"
    );
}

// ============================================================================
// unpause_app tests
// ============================================================================

#[test]
fn test_unpause_app_success() {
    let mollusk = setup_test_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_unpauser(admin, admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

    // App is paused
    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
        true, // paused
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(instructions_sysvar, false),
        ],
        data: crate::instruction::UnpauseApp {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (access_manager_pda, access_manager_account),
        (admin, create_signer_account()),
        (instructions_sysvar, instructions_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        !result.program_result.is_err(),
        "unpause_app should succeed: {:?}",
        result.program_result
    );

    // Verify app state is now unpaused
    let updated_account = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == app_state_pda)
        .expect("app state should exist")
        .1
        .clone();
    let updated_state = deserialize_app_state(&updated_account);
    assert!(!updated_state.paused, "App should be unpaused");
}

#[test]
fn test_unpause_app_not_paused_fails() {
    let mollusk = setup_test_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_unpauser(admin, admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

    // App is NOT paused
    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
        false, // not paused
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(instructions_sysvar, false),
        ],
        data: crate::instruction::UnpauseApp {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (access_manager_pda, access_manager_account),
        (admin, create_signer_account()),
        (instructions_sysvar, instructions_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "unpause_app should fail when not paused"
    );
}

// ============================================================================
// set_access_manager tests
// ============================================================================

#[test]
fn test_set_access_manager_success() {
    let mollusk = setup_test_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let new_access_manager = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_admin(admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
        false,
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(instructions_sysvar, false),
        ],
        data: crate::instruction::SetAccessManager { new_access_manager }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (access_manager_pda, access_manager_account),
        (admin, create_signer_account()),
        (instructions_sysvar, instructions_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        !result.program_result.is_err(),
        "set_access_manager should succeed: {:?}",
        result.program_result
    );

    // Verify access manager was updated
    let updated_account = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == app_state_pda)
        .expect("app state should exist")
        .1
        .clone();
    let updated_state = deserialize_app_state(&updated_account);
    assert_eq!(
        updated_state.access_manager, new_access_manager,
        "Access manager should be updated"
    );
}

#[test]
fn test_set_access_manager_unauthorized_fails() {
    let mollusk = setup_test_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let unauthorized = Pubkey::new_unique();
    let new_access_manager = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_admin(admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
        false,
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(unauthorized, true), // Not an admin
            AccountMeta::new_readonly(instructions_sysvar, false),
        ],
        data: crate::instruction::SetAccessManager { new_access_manager }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (access_manager_pda, access_manager_account),
        (unauthorized, create_signer_account()),
        (instructions_sysvar, instructions_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "set_access_manager should fail for unauthorized user"
    );
}
