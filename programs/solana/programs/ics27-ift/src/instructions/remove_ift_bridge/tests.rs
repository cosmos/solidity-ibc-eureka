//! Tests for `remove_ift_bridge` instruction

use anchor_lang::InstructionData;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::state::CounterpartyChainType;
use crate::test_utils::*;

#[test]
fn test_remove_ift_bridge_success() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, bridge_bump) = get_bridge_pda(&mint, client_id);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_admin(admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
    );

    // Bridge account exists
    let bridge_account = create_ift_bridge_account(
        mint,
        client_id,
        "0x1234",
        CounterpartyChainType::Evm,
        bridge_bump,
        true,
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(bridge_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::RemoveIftBridge {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (bridge_pda, bridge_account),
        (access_manager_pda, access_manager_account),
        (admin, create_signer_account()),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        !result.program_result.is_err(),
        "remove_ift_bridge should succeed: {:?}",
        result.program_result
    );

    // Verify bridge was closed (lamports zeroed, data cleared)
    let bridge_result = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == bridge_pda)
        .expect("bridge should exist")
        .1
        .clone();

    assert_eq!(
        bridge_result.lamports, 0,
        "Bridge lamports should be zero after close"
    );
}

#[test]
fn test_remove_ift_bridge_unauthorized_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let unauthorized = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, bridge_bump) = get_bridge_pda(&mint, client_id);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_admin(admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
    );

    let bridge_account = create_ift_bridge_account(
        mint,
        client_id,
        "0x1234",
        CounterpartyChainType::Evm,
        bridge_bump,
        true,
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(bridge_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(unauthorized, true), // Not admin
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::RemoveIftBridge {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (bridge_pda, bridge_account),
        (access_manager_pda, access_manager_account),
        (unauthorized, create_signer_account()),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "remove_ift_bridge should fail for unauthorized user"
    );
}

#[test]
fn test_remove_ift_bridge_mint_mismatch_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let wrong_mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    // Bridge PDA derived from correct mint (for seeds to match)
    let (bridge_pda, bridge_bump) = get_bridge_pda(&mint, client_id);
    let (access_manager_pda, access_manager_account) =
        create_access_manager_account_with_admin(admin);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
    );

    // Bridge account with WRONG mint stored inside
    let bridge_account = create_ift_bridge_account(
        wrong_mint, // Mismatched mint
        client_id,
        "0x1234",
        CounterpartyChainType::Evm,
        bridge_bump,
        true,
    );

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(bridge_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::RemoveIftBridge {}.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (bridge_pda, bridge_account),
        (access_manager_pda, access_manager_account),
        (admin, create_signer_account()),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (system_program, system_account),
    ];

    // Should fail with BridgeNotFound due to mint mismatch
    // Anchor error code: 6000 base + 7000 (IFT offset) + 4 (BridgeNotFound) = 13004
    let checks = vec![mollusk_svm::result::Check::err(
        solana_sdk::program_error::ProgramError::Custom(13004),
    )];
    mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
}
