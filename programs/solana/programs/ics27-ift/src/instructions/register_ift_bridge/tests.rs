//! Tests for `register_ift_bridge` instruction

use anchor_lang::{InstructionData, Space};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    rent::Rent,
};
use test_case::test_case;

use crate::state::{CounterpartyChainType, IFTBridge, RegisterIFTBridgeMsg};
use crate::test_utils::*;

const TEST_CLIENT_ID: &str = "07-tendermint-0";
const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";

#[test]
fn test_register_ift_bridge_success() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let payer = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, TEST_CLIENT_ID);
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
        client_id: TEST_CLIENT_ID.to_string(),
        counterparty_ift_address: TEST_COUNTERPARTY_ADDRESS.to_string(),
        counterparty_denom: String::new(), // Optional for EVM
        cosmos_type_url: String::new(),    // Optional for EVM
        cosmos_ica_address: String::new(), // Optional for EVM
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
    assert_eq!(bridge.client_id, TEST_CLIENT_ID);
    assert_eq!(bridge.counterparty_ift_address, TEST_COUNTERPARTY_ADDRESS);
    assert!(bridge.active);
}

/// Error scenarios for register_ift_bridge validation tests
#[derive(Clone, Copy)]
enum RegisterBridgeErrorCase {
    EmptyClientId,
    EmptyCounterparty,
    Unauthorized,
    ClientIdTooLong,
    CounterpartyTooLong,
}

/// Configuration derived from error case
struct RegisterBridgeTestConfig {
    client_id: String,
    counterparty_address: String,
    use_unauthorized_signer: bool,
}

impl From<RegisterBridgeErrorCase> for RegisterBridgeTestConfig {
    fn from(case: RegisterBridgeErrorCase) -> Self {
        match case {
            RegisterBridgeErrorCase::EmptyClientId => Self {
                client_id: String::new(),
                counterparty_address: "0x1234".to_string(),
                use_unauthorized_signer: false,
            },
            RegisterBridgeErrorCase::EmptyCounterparty => Self {
                client_id: TEST_CLIENT_ID.to_string(),
                counterparty_address: String::new(),
                use_unauthorized_signer: false,
            },
            RegisterBridgeErrorCase::Unauthorized => Self {
                client_id: TEST_CLIENT_ID.to_string(),
                counterparty_address: "0x1234".to_string(),
                use_unauthorized_signer: true,
            },
            RegisterBridgeErrorCase::ClientIdTooLong => Self {
                client_id: "x".repeat(crate::constants::MAX_CLIENT_ID_LENGTH + 1),
                counterparty_address: "0x1234".to_string(),
                use_unauthorized_signer: false,
            },
            RegisterBridgeErrorCase::CounterpartyTooLong => Self {
                client_id: TEST_CLIENT_ID.to_string(),
                counterparty_address: "x"
                    .repeat(crate::constants::MAX_COUNTERPARTY_ADDRESS_LENGTH + 1),
                use_unauthorized_signer: false,
            },
        }
    }
}

fn run_register_bridge_error_test(case: RegisterBridgeErrorCase) {
    let config = RegisterBridgeTestConfig::from(case);
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let unauthorized = Pubkey::new_unique();
    let payer = Pubkey::new_unique();

    // For PDA derivation, use a valid client_id if the test one is empty/too long
    let pda_client_id = if config.client_id.is_empty()
        || config.client_id.len() > crate::constants::MAX_CLIENT_ID_LENGTH
    {
        TEST_CLIENT_ID
    } else {
        &config.client_id
    };

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (bridge_pda, _) = get_bridge_pda(&mint, pda_client_id);
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
        client_id: config.client_id,
        counterparty_ift_address: config.counterparty_address,
        counterparty_denom: String::new(),
        cosmos_type_url: String::new(),
        cosmos_ica_address: String::new(),
        counterparty_chain_type: CounterpartyChainType::Evm,
    };

    // Select signer based on test case
    let signer = if config.use_unauthorized_signer {
        unauthorized
    } else {
        admin
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(bridge_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new_readonly(signer, true),
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
        (signer, create_signer_account()),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(result.program_result.is_err());
}

#[test_case(RegisterBridgeErrorCase::EmptyClientId ; "empty_client_id")]
#[test_case(RegisterBridgeErrorCase::EmptyCounterparty ; "empty_counterparty")]
#[test_case(RegisterBridgeErrorCase::Unauthorized ; "unauthorized")]
#[test_case(RegisterBridgeErrorCase::ClientIdTooLong ; "client_id_too_long")]
#[test_case(RegisterBridgeErrorCase::CounterpartyTooLong ; "counterparty_too_long")]
fn test_register_ift_bridge_validation(case: RegisterBridgeErrorCase) {
    run_register_bridge_error_test(case);
}
