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
use test_case::test_case;

const TEST_CLIENT_ID: &str = "07-tendermint-0";
const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";
const VALID_RECEIVER: &str = "0xabcdef1234567890abcdef1234567890abcdef12";

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
    // Short addresses should be rejected (must be exactly 20 bytes)
    assert!(construct_evm_mint_call("0xabcd", 100).is_err());
}

#[test]
fn test_construct_cosmos_mint_call() {
    let payload = construct_cosmos_mint_call(
        "/cosmos.ift.v1.MsgIFTMint",
        "cosmos1icaaddress",
        "uatom",
        "cosmos1abc123",
        1_000_000,
    );
    let json_str = String::from_utf8(payload).unwrap();

    // Should be wrapped in CosmosTx format with messages array
    assert!(json_str.contains("\"messages\":["));
    assert!(json_str.contains("\"@type\":\"/cosmos.ift.v1.MsgIFTMint\""));
    assert!(json_str.contains("\"signer\":\"cosmos1icaaddress\""));
    assert!(json_str.contains("\"denom\":\"uatom\""));
    assert!(json_str.contains("\"receiver\":\"cosmos1abc123\""));
    assert!(json_str.contains("\"amount\":\"1000000\""));
}

#[test]
fn test_construct_cosmos_mint_call_with_ibc_denom() {
    let payload = construct_cosmos_mint_call(
        "/wfchain.ift.MsgIFTMint",
        "wf1icaaddress",
        "ibc/ABC123",
        "cosmos1xyz",
        42,
    );
    let json_str = String::from_utf8(payload).unwrap();
    assert!(json_str.contains("\"denom\":\"ibc/ABC123\""));
    assert!(json_str.contains("\"@type\":\"/wfchain.ift.MsgIFTMint\""));
    assert!(json_str.contains("\"signer\":\"wf1icaaddress\""));
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
        "", // denom not used for EVM
        "", // cosmos_type_url not used for EVM
        "", // cosmos_ica_address not used for EVM
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
        "cosmos1iftmodule", // counterparty_ift_address (not used in payload)
        "uatom",            // counterparty_denom (used as denom in MsgIFTMint)
        "/cosmos.ift.v1.MsgIFTMint", // cosmos_type_url
        "cosmos1icaaddress", // cosmos_ica_address (used as signer in MsgIFTMint)
        "cosmos1receiver",
        100,
    );
    assert!(result.is_ok());
    let json = String::from_utf8(result.unwrap()).unwrap();
    assert!(json.contains("/cosmos.ift.v1.MsgIFTMint"));
    assert!(json.contains("uatom")); // Verify denom is in payload
    assert!(json.contains("cosmos1icaaddress")); // Verify signer is in payload
}

#[test]
fn test_construct_mint_call_solana() {
    // Use valid base58 pubkey (system program)
    let result = construct_mint_call(
        CounterpartyChainType::Solana,
        "ignored",
        "", // denom not used for Solana
        "", // cosmos_type_url not used for Solana
        "", // cosmos_ica_address not used for Solana
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

/// Mint layout: `authority_option` (4) + authority (32) + supply (8) + decimals (1) + `is_initialized` (1)
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

/// Error scenarios for IFT transfer validation tests
#[derive(Clone, Copy)]
enum TransferErrorCase {
    InactiveBridge,
    ZeroAmount,
    EmptyReceiver,
    SenderNotSigner,
    WrongTokenAccountOwner,
    WrongTokenMint,
    TimeoutInPast,
    TimeoutTooLong,
    ReceiverTooLong,
}

/// Configuration derived from error case
#[allow(
    clippy::struct_excessive_bools,
    reason = "test config uses bools for clarity"
)]
struct TransferTestConfig {
    bridge_active: bool,
    amount: u64,
    receiver: String,
    sender_is_signer: bool,
    use_wrong_token_owner: bool,
    use_wrong_token_mint: bool,
    timeout_timestamp: i64,
}

impl From<TransferErrorCase> for TransferTestConfig {
    fn from(case: TransferErrorCase) -> Self {
        let default = Self {
            bridge_active: true,
            amount: 1000,
            receiver: VALID_RECEIVER.to_string(),
            sender_is_signer: true,
            use_wrong_token_owner: false,
            use_wrong_token_mint: false,
            timeout_timestamp: 0, // Use default
        };

        match case {
            TransferErrorCase::InactiveBridge => Self {
                bridge_active: false,
                ..default
            },
            TransferErrorCase::ZeroAmount => Self {
                amount: 0,
                ..default
            },
            TransferErrorCase::EmptyReceiver => Self {
                receiver: String::new(),
                ..default
            },
            TransferErrorCase::SenderNotSigner => Self {
                sender_is_signer: false,
                ..default
            },
            TransferErrorCase::WrongTokenAccountOwner => Self {
                use_wrong_token_owner: true,
                ..default
            },
            TransferErrorCase::WrongTokenMint => Self {
                use_wrong_token_mint: true,
                ..default
            },
            TransferErrorCase::TimeoutInPast => Self {
                timeout_timestamp: 1, // Way in the past
                ..default
            },
            TransferErrorCase::TimeoutTooLong => Self {
                timeout_timestamp: i64::MAX, // Way in the future
                ..default
            },
            TransferErrorCase::ReceiverTooLong => Self {
                receiver: "x".repeat(crate::constants::MAX_RECEIVER_LENGTH + 1),
                ..default
            },
        }
    }
}

fn run_transfer_error_test(case: TransferErrorCase) {
    let config = TransferTestConfig::from(case);
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let wrong_mint = Pubkey::new_unique();
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
        "",
        "",
        "",
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        config.bridge_active,
    );

    let mint_account = create_mint_account(None);
    let sender_token_pda = Pubkey::new_unique();

    // Token account configuration based on test case
    let token_account_owner = if config.use_wrong_token_owner {
        wrong_owner
    } else {
        sender
    };
    let token_account_mint = if config.use_wrong_token_mint {
        wrong_mint
    } else {
        mint
    };
    let sender_token_account =
        create_token_account(&token_account_mint, &token_account_owner, 10000);

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
        receiver: config.receiver,
        amount: config.amount,
        timeout_timestamp: config.timeout_timestamp,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_token_pda, false),
            AccountMeta::new_readonly(sender, config.sender_is_signer),
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
    assert!(result.program_result.is_err());
}

#[test_case(TransferErrorCase::InactiveBridge ; "inactive_bridge")]
#[test_case(TransferErrorCase::ZeroAmount ; "zero_amount")]
#[test_case(TransferErrorCase::EmptyReceiver ; "empty_receiver")]
#[test_case(TransferErrorCase::SenderNotSigner ; "sender_not_signer")]
#[test_case(TransferErrorCase::WrongTokenAccountOwner ; "wrong_token_account_owner")]
#[test_case(TransferErrorCase::WrongTokenMint ; "wrong_token_mint")]
#[test_case(TransferErrorCase::TimeoutInPast ; "timeout_in_past")]
#[test_case(TransferErrorCase::TimeoutTooLong ; "timeout_too_long")]
#[test_case(TransferErrorCase::ReceiverTooLong ; "receiver_too_long")]
fn test_ift_transfer_validation(case: TransferErrorCase) {
    run_transfer_error_test(case);
}
