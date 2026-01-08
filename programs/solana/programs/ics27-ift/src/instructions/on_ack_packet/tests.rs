//! Tests for acknowledgement packet handling

use super::*;
use sha2::{Digest, Sha256};

use anchor_lang::InstructionData;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::test_utils::*;

fn universal_error_ack() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"UNIVERSAL_ERROR_ACKNOWLEDGEMENT");
    let result = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    bytes
}

#[test]
fn test_parse_gmp_acknowledgement_error() {
    let error_ack = universal_error_ack();
    assert!(
        !parse_gmp_acknowledgement(&error_ack),
        "UNIVERSAL_ERROR_ACK should indicate failure"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_success() {
    let success_ack = b"some protobuf encoded acknowledgement";
    assert!(
        parse_gmp_acknowledgement(success_ack),
        "Non-error ack should indicate success"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_empty() {
    let empty_ack: &[u8] = &[];
    assert!(
        parse_gmp_acknowledgement(empty_ack),
        "Empty ack should indicate success (not error)"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_partial_match() {
    let error_ack = universal_error_ack();
    let partial_ack = &error_ack[..31];
    assert!(
        parse_gmp_acknowledgement(partial_ack),
        "Partial match should indicate success"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_extended() {
    let error_ack = universal_error_ack();
    let mut extended = error_ack.to_vec();
    extended.push(0);
    assert!(
        parse_gmp_acknowledgement(&extended),
        "Extended error ack should indicate success"
    );
}

// ============================================================================
// on_ack_packet instruction tests
// ============================================================================

#[test]
fn test_on_ack_packet_direct_call_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    let sequence = 1u64;

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (pending_pda, pending_bump) = get_pending_transfer_pda(&mint, client_id, sequence);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
    );

    let pending_account =
        create_pending_transfer_account(mint, client_id, sequence, sender, 1000, pending_bump);

    // Create mock mint account
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

    // Create mock sender token account
    let sender_token_account_pda = Pubkey::new_unique();
    let mut sender_token_data = vec![0u8; 165];
    sender_token_data[0..32].copy_from_slice(&mint.to_bytes());
    sender_token_data[32..64].copy_from_slice(&sender.to_bytes());
    let sender_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: sender_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let router_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let msg = solana_ibc_types::OnAcknowledgementPacketMsg {
        source_client: client_id.to_string(),
        dest_client: "client-1".to_string(),
        sequence,
        acknowledgement: b"success".to_vec(), // success ack
        payload: solana_ibc_types::Payload {
            source_port: "gmp".to_string(),
            dest_port: "gmp".to_string(),
            version: "1".to_string(),
            encoding: "proto".to_string(),
            value: vec![],
        },
        relayer: payer,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(pending_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_token_account_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::OnAcknowledgementPacket { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (pending_pda, pending_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (sender_token_account_pda, sender_token_account),
        (ics26_router::ID, router_program_account),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "on_ack_packet should fail when called directly (not via CPI from router)"
    );
}

#[test]
fn test_on_ack_packet_wrong_token_owner_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let wrong_owner = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    let sequence = 1u64;

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (pending_pda, pending_bump) = get_pending_transfer_pda(&mint, client_id, sequence);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (instructions_sysvar, instructions_account) =
        create_instructions_sysvar_account_with_caller(ics26_router::ID);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
    );

    let pending_account =
        create_pending_transfer_account(mint, client_id, sequence, sender, 1000, pending_bump);

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

    // Token account with WRONG owner (not sender)
    let sender_token_account_pda = Pubkey::new_unique();
    let mut sender_token_data = vec![0u8; 165];
    sender_token_data[0..32].copy_from_slice(&mint.to_bytes());
    sender_token_data[32..64].copy_from_slice(&wrong_owner.to_bytes()); // Wrong owner!
    let sender_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: sender_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let router_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let msg = solana_ibc_types::OnAcknowledgementPacketMsg {
        source_client: client_id.to_string(),
        dest_client: "client-1".to_string(),
        sequence,
        acknowledgement: b"success".to_vec(),
        payload: solana_ibc_types::Payload {
            source_port: "gmp".to_string(),
            dest_port: "gmp".to_string(),
            version: "1".to_string(),
            encoding: "proto".to_string(),
            value: vec![],
        },
        relayer: payer,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(pending_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_token_account_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::OnAcknowledgementPacket { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (pending_pda, pending_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (sender_token_account_pda, sender_token_account),
        (ics26_router::ID, router_program_account),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "on_ack_packet should fail with wrong token account owner"
    );
}

#[test]
fn test_on_ack_packet_wrong_token_mint_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let wrong_mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    let sequence = 1u64;

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    let (pending_pda, pending_bump) = get_pending_transfer_pda(&mint, client_id, sequence);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (instructions_sysvar, instructions_account) =
        create_instructions_sysvar_account_with_caller(ics26_router::ID);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
    );

    let pending_account =
        create_pending_transfer_account(mint, client_id, sequence, sender, 1000, pending_bump);

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

    // Token account with WRONG mint
    let sender_token_account_pda = Pubkey::new_unique();
    let mut sender_token_data = vec![0u8; 165];
    sender_token_data[0..32].copy_from_slice(&wrong_mint.to_bytes()); // Wrong mint!
    sender_token_data[32..64].copy_from_slice(&sender.to_bytes());
    let sender_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: sender_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let router_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let msg = solana_ibc_types::OnAcknowledgementPacketMsg {
        source_client: client_id.to_string(),
        dest_client: "client-1".to_string(),
        sequence,
        acknowledgement: b"success".to_vec(),
        payload: solana_ibc_types::Payload {
            source_port: "gmp".to_string(),
            dest_port: "gmp".to_string(),
            version: "1".to_string(),
            encoding: "proto".to_string(),
            value: vec![],
        },
        relayer: payer,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(pending_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_token_account_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::OnAcknowledgementPacket { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (pending_pda, pending_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (sender_token_account_pda, sender_token_account),
        (ics26_router::ID, router_program_account),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "on_ack_packet should fail with wrong mint in token account"
    );
}

#[test]
fn test_on_ack_packet_pending_mint_mismatch_fails() {
    let mollusk = setup_mollusk();

    let mint = Pubkey::new_unique();
    let wrong_mint = Pubkey::new_unique();
    let sender = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    let sequence = 1u64;

    let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
    let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
    // Pending transfer uses wrong_mint (mismatches app_state.mint)
    let (pending_pda, pending_bump) = get_pending_transfer_pda(&wrong_mint, client_id, sequence);
    let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
    let (instructions_sysvar, instructions_account) =
        create_instructions_sysvar_account_with_caller(ics26_router::ID);
    let (system_program, system_account) = create_system_program_account();

    let app_state_account = create_ift_app_state_account(
        mint,
        app_state_bump,
        mint_authority_bump,
        access_manager::ID,
        Pubkey::new_unique(),
    );

    // Pending transfer with wrong mint
    let pending_account = create_pending_transfer_account(
        wrong_mint, // Wrong mint!
        client_id,
        sequence,
        sender,
        1000,
        pending_bump,
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

    let sender_token_account_pda = Pubkey::new_unique();
    let mut sender_token_data = vec![0u8; 165];
    sender_token_data[0..32].copy_from_slice(&mint.to_bytes());
    sender_token_data[32..64].copy_from_slice(&sender.to_bytes());
    let sender_token_account = solana_sdk::account::Account {
        lamports: 1_000_000,
        data: sender_token_data,
        owner: anchor_spl::token::ID,
        executable: false,
        rent_epoch: 0,
    };

    let router_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let token_program_account = solana_sdk::account::Account {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    };

    let msg = solana_ibc_types::OnAcknowledgementPacketMsg {
        source_client: client_id.to_string(),
        dest_client: "client-1".to_string(),
        sequence,
        acknowledgement: b"success".to_vec(),
        payload: solana_ibc_types::Payload {
            source_port: "gmp".to_string(),
            dest_port: "gmp".to_string(),
            version: "1".to_string(),
            encoding: "proto".to_string(),
            value: vec![],
        },
        relayer: payer,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(pending_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_token_account_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(instructions_sysvar, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(anchor_spl::token::ID, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: crate::instruction::OnAcknowledgementPacket { msg }.data(),
    };

    let accounts = vec![
        (app_state_pda, app_state_account),
        (pending_pda, pending_account),
        (mint, mint_account),
        (mint_authority_pda, mint_authority_account),
        (sender_token_account_pda, sender_token_account),
        (ics26_router::ID, router_program_account),
        (instructions_sysvar, instructions_account),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(
        result.program_result.is_err(),
        "on_ack_packet should fail when pending transfer mint doesn't match app_state.mint"
    );
}
