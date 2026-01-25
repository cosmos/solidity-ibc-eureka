//! Tests for `register_ift_bridge` instruction

use anchor_lang::{InstructionData, Space};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    rent::Rent,
};

use crate::state::{CounterpartyChainType, IFTBridge, RegisterIFTBridgeMsg};
use crate::test_utils::*;

#[test]
fn test_register_ift_bridge_success() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    let counterparty_address = "0x1234567890abcdef1234567890abcdef12345678";

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, client_id);
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

    // Bridge account starts uninitialized (with rent for account creation)
    let bridge_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTBridge::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let msg = RegisterIFTBridgeMsg {
        client_id: client_id.to_string(),
        counterparty_ift_address: counterparty_address.to_string(),
        counterparty_denom: String::new(), // Optional for EVM
        counterparty_chain_type: CounterpartyChainType::Evm,
    };

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
        data: crate::instruction::RegisterIftBridge { msg }.data(),
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
        "register_ift_bridge should succeed: {:?}",
        result.program_result
    );

    // Verify bridge was created
    let bridge_account = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == bridge_pda)
        .expect("bridge should exist")
        .1
        .clone();

    assert_eq!(
        bridge_account.owner,
        crate::ID,
        "Bridge should be owned by IFT program"
    );
    let bridge = deserialize_bridge(&bridge_account);
    assert_eq!(bridge.client_id, client_id);
    assert_eq!(bridge.counterparty_ift_address, counterparty_address);
    assert!(bridge.active);
}

#[test]
fn test_register_ift_bridge_empty_client_id_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = ""; // Empty!

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, client_id);
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

    let bridge_account = create_uninitialized_pda();

    let msg = RegisterIFTBridgeMsg {
        client_id: client_id.to_string(),
        counterparty_ift_address: "0x1234".to_string(),
        counterparty_denom: String::new(), // Optional for EVM
        counterparty_chain_type: CounterpartyChainType::Evm,
    };

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
        data: crate::instruction::RegisterIftBridge { msg }.data(),
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
        result.program_result.is_err(),
        "register_ift_bridge should fail with empty client_id"
    );
}

#[test]
fn test_register_ift_bridge_empty_counterparty_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, client_id);
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

    let bridge_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTBridge::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let msg = RegisterIFTBridgeMsg {
        client_id: client_id.to_string(),
        counterparty_ift_address: String::new(), // Empty!
        counterparty_denom: String::new(),
        counterparty_chain_type: CounterpartyChainType::Evm,
    };

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
        data: crate::instruction::RegisterIftBridge { msg }.data(),
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
        result.program_result.is_err(),
        "register_ift_bridge should fail with empty counterparty address"
    );
}

#[test]
fn test_register_ift_bridge_unauthorized_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let unauthorized = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, client_id);
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

    let bridge_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTBridge::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let msg = RegisterIFTBridgeMsg {
        client_id: client_id.to_string(),
        counterparty_ift_address: "0x1234".to_string(),
        counterparty_denom: String::new(), // Optional for EVM
        counterparty_chain_type: CounterpartyChainType::Evm,
    };

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
        data: crate::instruction::RegisterIftBridge { msg }.data(),
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
        "register_ift_bridge should fail for unauthorized user"
    );
}

#[test]
fn test_register_ift_bridge_client_id_too_long_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    // Exceeds MAX_CLIENT_ID_LENGTH (64)
    let long_client_id = "x".repeat(crate::constants::MAX_CLIENT_ID_LENGTH + 1);
    // Use a valid short client_id for PDA derivation (length check should fail before PDA validation)
    let valid_client_id = "07-tendermint-0";

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, valid_client_id);
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

    let bridge_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTBridge::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let msg = RegisterIFTBridgeMsg {
        client_id: long_client_id,
        counterparty_ift_address: "0x1234".to_string(),
        counterparty_denom: String::new(),
        counterparty_chain_type: CounterpartyChainType::Evm,
    };

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
        data: crate::instruction::RegisterIftBridge { msg }.data(),
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
        result.program_result.is_err(),
        "register_ift_bridge should fail when client_id exceeds max length"
    );
}

#[test]
fn test_register_ift_bridge_counterparty_too_long_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    // Exceeds MAX_COUNTERPARTY_ADDRESS_LENGTH (128)
    let counterparty_address = "x".repeat(crate::constants::MAX_COUNTERPARTY_ADDRESS_LENGTH + 1);

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, client_id);
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

    let bridge_account = solana_sdk::account::Account {
        lamports: Rent::default().minimum_balance(8 + IFTBridge::INIT_SPACE),
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    };

    let msg = RegisterIFTBridgeMsg {
        client_id: client_id.to_string(),
        counterparty_ift_address: counterparty_address,
        counterparty_denom: String::new(),
        counterparty_chain_type: CounterpartyChainType::Evm,
    };

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
        data: crate::instruction::RegisterIftBridge { msg }.data(),
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
        result.program_result.is_err(),
        "register_ift_bridge should fail when counterparty address exceeds max length"
    );
}
