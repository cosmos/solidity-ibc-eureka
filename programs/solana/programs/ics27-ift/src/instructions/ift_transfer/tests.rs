//! Tests for IFT transfer payload construction and instruction

use super::*;
use crate::evm_selectors::{IFT_MINT_DISCRIMINATOR, IFT_MINT_SELECTOR};
use crate::state::IFTTransferMsg;
use crate::test_utils::*;
use anchor_lang::InstructionData;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

const TEST_CLIENT_ID: &str = "07-tendermint-0";
const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";

#[test]
fn test_construct_evm_mint_call_basic() {
    let receiver = "0x1234567890abcdef1234567890abcdef12345678";
    let amount = 1_000_000u64;

    let payload = construct_evm_mint_call(receiver, amount).unwrap();

    // Should be 4 (selector) + 32 (address) + 32 (amount) = 68 bytes
    assert_eq!(payload.len(), 68);
    assert_eq!(&payload[0..4], &IFT_MINT_SELECTOR);

    // Address should be left-padded to 32 bytes (12 zero bytes + 20 address bytes)
    assert_eq!(&payload[4..16], &[0u8; 12]);

    // Amount should be in last 32 bytes, big-endian, left-padded
    let amount_bytes = &payload[36..68];
    assert_eq!(&amount_bytes[0..24], &[0u8; 24]);
    assert_eq!(&amount_bytes[24..32], &amount.to_be_bytes());
}

#[test]
fn test_construct_evm_mint_call_without_0x_prefix() {
    let receiver = "1234567890abcdef1234567890abcdef12345678";
    let payload = construct_evm_mint_call(receiver, 500).unwrap();
    assert_eq!(payload.len(), 68);
}

#[test]
fn test_construct_evm_mint_call_max_amount() {
    let receiver = "0xffffffffffffffffffffffffffffffffffffffff";
    let payload = construct_evm_mint_call(receiver, u64::MAX).unwrap();
    let amount_bytes = &payload[36..68];
    assert_eq!(&amount_bytes[24..32], &u64::MAX.to_be_bytes());
}

#[test]
fn test_construct_evm_mint_call_invalid_hex() {
    assert!(construct_evm_mint_call("0xnothex", 100).is_err());
}

#[test]
fn test_construct_evm_mint_call_short_address() {
    let payload = construct_evm_mint_call("0xabcd", 100).unwrap();
    assert_eq!(payload.len(), 68);
    assert_eq!(&payload[4..34], &[0u8; 30]);
}

#[test]
fn test_construct_cosmos_mint_call() {
    let payload = construct_cosmos_mint_call("uatom", "cosmos1abc123", 1_000_000);
    let json_str = String::from_utf8(payload).unwrap();

    assert!(json_str.contains("\"@type\":\"/cosmos.ift.v1.MsgIFTMint\""));
    assert!(json_str.contains("\"denom\":\"uatom\""));
    assert!(json_str.contains("\"receiver\":\"cosmos1abc123\""));
    assert!(json_str.contains("\"amount\":\"1000000\""));
}

#[test]
fn test_construct_cosmos_mint_call_with_ibc_denom() {
    let payload = construct_cosmos_mint_call("ibc/ABC123", "cosmos1xyz", 42);
    let json_str = String::from_utf8(payload).unwrap();
    assert!(json_str.contains("\"denom\":\"ibc/ABC123\""));
}

#[test]
fn test_construct_solana_mint_call() {
    // Valid base58-encoded pubkey (system program)
    let receiver = "11111111111111111111111111111111";
    let amount = 999u64;

    let payload = construct_solana_mint_call(receiver, amount).unwrap();

    // 8 bytes discriminator + 32 bytes pubkey + 8 bytes amount
    assert_eq!(payload.len(), 48);
    assert_eq!(&payload[0..8], &IFT_MINT_DISCRIMINATOR);
    assert_eq!(&payload[40..48], &amount.to_le_bytes());
}

#[test]
fn test_construct_solana_mint_call_invalid_pubkey() {
    let invalid_receiver = "not-a-valid-base58-pubkey";
    let amount = 999u64;

    let result = construct_solana_mint_call(invalid_receiver, amount);
    assert!(result.is_err());
}

#[test]
fn test_construct_mint_call_evm() {
    let result = construct_mint_call(
        CounterpartyChainType::Evm,
        "ignored",
        "0x1234567890abcdef1234567890abcdef12345678",
        100,
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 68);
}

#[test]
fn test_construct_mint_call_cosmos() {
    let result = construct_mint_call(
        CounterpartyChainType::Cosmos,
        "uatom",
        "cosmos1receiver",
        100,
    );
    assert!(result.is_ok());
    let json = String::from_utf8(result.unwrap()).unwrap();
    assert!(json.contains("MsgIFTMint"));
}

#[test]
fn test_construct_mint_call_solana() {
    // Use valid base58 pubkey (system program)
    let result = construct_mint_call(
        CounterpartyChainType::Solana,
        "ignored",
        "11111111111111111111111111111111",
        100,
    );
    assert!(result.is_ok());
    // 8 bytes discriminator + 32 bytes pubkey + 8 bytes amount
    assert_eq!(result.unwrap().len(), 48);
}

/// Token account layout: mint (32) + owner (32) + amount (8) + ... + state (1 @ offset 108)
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

/// Mint layout: authority_option (4) + authority (32) + supply (8) + decimals (1) + is_initialized (1)
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

fn build_ift_transfer_test_setup(
    bridge_active: bool,
    token_amount: u64,
) -> (
    Instruction,
    Vec<(Pubkey, solana_sdk::account::Account)>,
    Pubkey, // mint
    Pubkey, // sender
) {
    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();
    let router_program = ics26_router::ID;

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
    let (clock_sysvar, clock_account) = create_clock_sysvar_account(1_700_000_000);

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        bridge_active,
    );

    let mint_account = create_mint_account(None);
    let sender_token_account = create_token_account(&mint, &sender, token_amount);
    let sender_token_pda = Pubkey::new_unique();

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    // GMP app state PDA
    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    // Router state - mock account
    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();

    // Pending transfer will be created with sequence from router - use placeholder for now
    // In real execution, sequence comes from router CPI
    let pending_transfer = Pubkey::new_unique();

    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
        amount: 1000,
        timeout_timestamp: 0, // Use default
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(router_program, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (router_program, token_program_account.clone()), // executable
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
        (clock_sysvar, clock_account),
    ];

    (instruction, accounts, mint, sender)
}

/// Test that `ift_transfer` fails when bridge is not active
#[test]
fn test_ift_transfer_inactive_bridge_fails() {
    let mollusk = setup_mollusk();

    let (instruction, accounts, _, _) = build_ift_transfer_test_setup(
        false, // bridge NOT active
        10000, // token amount
    );

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail when bridge is not active"
    );
}

/// Test that `ift_transfer` fails with zero amount
#[test]
fn test_ift_transfer_zero_amount_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &sender, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    // Zero amount!
    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
        amount: 0, // ZERO!
        timeout_timestamp: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail with zero amount"
    );
}

/// Test that `ift_transfer` fails with empty receiver
#[test]
fn test_ift_transfer_empty_receiver_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &sender, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    // Empty receiver!
    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: String::new(), // EMPTY!
        amount: 1000,
        timeout_timestamp: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail with empty receiver"
    );
}

/// Test that `ift_transfer` fails when sender is not a signer
#[test]
fn test_ift_transfer_sender_not_signer_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &sender, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
        amount: 1000,
        timeout_timestamp: 0,
    };

    // Sender is NOT a signer!
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, false), // NOT a signer!
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail when sender is not a signer"
    );
}

/// Test that `ift_transfer` fails when token account owner doesn't match sender
#[test]
fn test_ift_transfer_wrong_token_account_owner_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let wrong_owner = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    // Token account owned by wrong_owner, not sender!
    let sender_token_account = create_token_account(&mint, &wrong_owner, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
        amount: 1000,
        timeout_timestamp: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail when token account owner doesn't match sender"
    );
}

/// Test that `ift_transfer` fails when token account mint doesn't match
#[test]
fn test_ift_transfer_wrong_token_mint_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let wrong_mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    // Token account for wrong_mint, not mint!
    let sender_token_account = create_token_account(&wrong_mint, &sender, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
        amount: 1000,
        timeout_timestamp: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail when token account mint doesn't match"
    );
}

/// Test that `ift_transfer` fails with timeout in the past
#[test]
fn test_ift_transfer_timeout_in_past_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &sender, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    // Timeout in the past!
    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
        amount: 1000,
        timeout_timestamp: 1, // Very old timestamp
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail with timeout in the past"
    );
}

/// Test that `ift_transfer` fails with timeout too far in the future
#[test]
fn test_ift_transfer_timeout_too_long_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
    let (clock_sysvar, clock_account) = create_clock_sysvar_account(1_700_000_000);

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &sender, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    // Timeout too far in future (> 24 hours from clock time 1_700_000_000)
    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
        amount: 1000,
        timeout_timestamp: 1_700_000_000 + crate::constants::MAX_TIMEOUT_DURATION + 1,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
        (clock_sysvar, clock_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail with timeout too far in future"
    );
}

/// Test that `ift_transfer` fails when receiver exceeds max length
#[test]
fn test_ift_transfer_receiver_too_long_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let gmp_program = Pubkey::new_unique();

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
    let (system_program, system_account) = create_system_program_account();
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

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
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        true,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();
    let sender_token_account = create_token_account(&mint, &sender, 10000);

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let (gmp_app_state_pda, _) = Pubkey::find_program_address(
        &[
            solana_ibc_types::GMPAppState::SEED,
            ics27_gmp::constants::GMP_PORT_ID.as_bytes(),
        ],
        &gmp_program,
    );

    let router_state = Pubkey::new_unique();
    let client_sequence = Pubkey::new_unique();
    let packet_commitment = Pubkey::new_unique();
    let gmp_ibc_app = Pubkey::new_unique();
    let ibc_client = Pubkey::new_unique();
    let pending_transfer = Pubkey::new_unique();

    // Receiver too long (> 128 chars)
    let long_receiver = "x".repeat(crate::constants::MAX_RECEIVER_LENGTH + 1);
    let msg = IFTTransferMsg {
        client_id: TEST_CLIENT_ID.to_string(),
        receiver: long_receiver,
        amount: 1000,
        timeout_timestamp: 0,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new_readonly(gmp_ibc_app, false),
            AccountMeta::new_readonly(ibc_client, false),
            AccountMeta::new(pending_transfer, false),
        ],
        data: crate::instruction::IftTransfer { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (ift_bridge_pda, ift_bridge_account),
        (mint, mint_account),
        (sender_token_pda, sender_token_account),
        (sender, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account.clone()),
        (system_program, system_account),
        (gmp_program, create_gmp_program_account()),
        (gmp_app_state_pda, create_signer_account()),
        (ics26_router::ID, token_program_account),
        (router_state, create_signer_account()),
        (client_sequence, create_signer_account()),
        (packet_commitment, create_uninitialized_pda()),
        (instructions_sysvar, instructions_account),
        (gmp_ibc_app, create_signer_account()),
        (ibc_client, create_signer_account()),
        (pending_transfer, create_uninitialized_pda()),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "ift_transfer should fail when receiver exceeds max length"
    );
}
