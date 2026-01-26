//! Tests for `ift_mint` instruction

use anchor_lang::InstructionData;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use test_case::test_case;

use crate::state::{CounterpartyChainType, IFTMintMsg};
use crate::test_utils::*;

const TEST_CLIENT_ID: &str = "07-tendermint-0";
const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";

/// Error scenarios for IFT mint validation tests
#[derive(Clone, Copy)]
enum MintErrorCase {
    ZeroAmount,
    ReceiverMismatch,
    GmpNotSigner,
    BridgeNotActive,
    InvalidGmpAccount,
}

/// Configuration derived from error case
#[allow(
    clippy::struct_excessive_bools,
    reason = "test config uses bools for clarity"
)]
struct MintTestConfig {
    amount: u64,
    use_wrong_receiver: bool,
    gmp_is_signer: bool,
    bridge_active: bool,
    use_wrong_gmp_account: bool,
}

impl From<MintErrorCase> for MintTestConfig {
    fn from(case: MintErrorCase) -> Self {
        match case {
            MintErrorCase::ZeroAmount => Self {
                amount: 0,
                use_wrong_receiver: false,
                gmp_is_signer: true,
                bridge_active: true,
                use_wrong_gmp_account: false,
            },
            MintErrorCase::ReceiverMismatch => Self {
                amount: 1000,
                use_wrong_receiver: true,
                gmp_is_signer: true,
                bridge_active: true,
                use_wrong_gmp_account: false,
            },
            MintErrorCase::GmpNotSigner => Self {
                amount: 1000,
                use_wrong_receiver: false,
                gmp_is_signer: false,
                bridge_active: true,
                use_wrong_gmp_account: false,
            },
            MintErrorCase::BridgeNotActive => Self {
                amount: 1000,
                use_wrong_receiver: false,
                gmp_is_signer: true,
                bridge_active: false,
                use_wrong_gmp_account: false,
            },
            MintErrorCase::InvalidGmpAccount => Self {
                amount: 1000,
                use_wrong_receiver: false,
                gmp_is_signer: true,
                bridge_active: true,
                use_wrong_gmp_account: true,
            },
        }
    }
}

fn run_mint_error_test(case: MintErrorCase) {
    let config = MintTestConfig::from(case);
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
        "",
        "",
        "",
        CounterpartyChainType::Evm,
        ift_bridge_bump,
        config.bridge_active,
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

    // Token account owner depends on test case
    let token_account_owner = if config.use_wrong_receiver {
        wrong_receiver
    } else {
        receiver
    };

    let receiver_token_pda = Pubkey::new_unique();
    let mut receiver_token_data = vec![0u8; 165];
    receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
    receiver_token_data[32..64].copy_from_slice(&token_account_owner.to_bytes());
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

    // Select GMP account based on test case
    let (gmp_account_key, gmp_bump) = if config.use_wrong_gmp_account {
        (wrong_gmp_account, 255)
    } else {
        (gmp_account_pda, gmp_account_bump)
    };

    // Select receiver_owner based on test case
    let receiver_owner_key = if config.use_wrong_receiver {
        wrong_receiver
    } else {
        receiver
    };

    let msg = IFTMintMsg {
        receiver,
        amount: config.amount,
        client_id: TEST_CLIENT_ID.to_string(),
        gmp_account_bump: gmp_bump,
    };

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(ift_bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(receiver_token_pda, false),
            AccountMeta::new_readonly(receiver_owner_key, false),
            AccountMeta::new_readonly(gmp_program, false),
            AccountMeta::new_readonly(gmp_account_key, config.gmp_is_signer),
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
        (receiver_owner_key, create_signer_account()),
        (gmp_program, create_gmp_program_account()),
        (gmp_account_key, create_signer_account()),
        (payer, create_signer_account()),
        (anchor_spl::token::ID, token_program_account),
        (
            anchor_spl::associated_token::ID,
            associated_token_program_account,
        ),
        (system_program, system_account),
    ];

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(result.program_result.is_err());
}

#[test_case(MintErrorCase::ZeroAmount ; "zero_amount")]
#[test_case(MintErrorCase::ReceiverMismatch ; "receiver_mismatch")]
#[test_case(MintErrorCase::GmpNotSigner ; "gmp_not_signer")]
#[test_case(MintErrorCase::BridgeNotActive ; "bridge_not_active")]
#[test_case(MintErrorCase::InvalidGmpAccount ; "invalid_gmp_account")]
fn test_ift_mint_validation(case: MintErrorCase) {
    run_mint_error_test(case);
}
