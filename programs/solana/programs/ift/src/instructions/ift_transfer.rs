use alloy_sol_types::SolCall;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::Space;
use anchor_spl::token_interface::{self, Burn, Mint, TokenAccount, TokenInterface};
use ics27_gmp::constants::GMP_PORT_ID;
use serde::Serialize;

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTTransferInitiated;
use crate::gmp_cpi::{SendGmpCallAccounts, SendGmpCallMsg};
use crate::state::{
    AccountVersion, ChainOptions, IFTAppState, IFTBridge, IFTTransferMsg, PendingTransfer,
};

#[derive(Accounts)]
#[instruction(msg: IFTTransferMsg)]
pub struct IFTTransfer<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
        constraint = !app_state.paused @ IFTError::TokenPaused,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge for the destination
    #[account(
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), msg.client_id.as_bytes()],
        bump = ift_bridge.bump,
        constraint = ift_bridge.active @ IFTError::BridgeNotActive
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// SPL Token mint
    #[account(
        mut,
        address = app_state.mint
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Sender's token account
    #[account(
        mut,
        // TODO: add its own error
        constraint = sender_token_account.mint == mint.key() @ IFTError::TokenAccountOwnerMismatch,
        constraint = sender_token_account.owner == sender.key() @ IFTError::TokenAccountOwnerMismatch
    )]
    pub sender_token_account: InterfaceAccount<'info, TokenAccount>,

    /// Sender who owns the tokens
    pub sender: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    /// Required for burning tokens from sender's account
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,

    /// GMP program
    /// CHECK: Validated against stored `gmp_program` in `app_state`
    #[account(
        address = app_state.gmp_program @ IFTError::InvalidGmpProgram
    )]
    pub gmp_program: AccountInfo<'info>,

    /// GMP app state PDA
    /// CHECK: Validated by GMP program via CPI
    #[account(
        mut,
        seeds = [solana_ibc_types::GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump,
        seeds::program = gmp_program.key()
    )]
    pub gmp_app_state: AccountInfo<'info>,

    /// Router program
    pub router_program: Program<'info, ics26_router::program::Ics26Router>,

    /// Router state account
    /// CHECK: Router program validates this
    #[account()]
    pub router_state: AccountInfo<'info>,

    /// Client sequence account for packet sequencing
    /// CHECK: Router program validates this
    #[account(mut)]
    pub client_sequence: AccountInfo<'info>,

    /// Packet commitment account to be created
    /// CHECK: Router program validates this
    #[account(mut)]
    pub packet_commitment: AccountInfo<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    /// GMP's IBC app registration account
    /// CHECK: Router program validates this
    #[account()]
    pub gmp_ibc_app: AccountInfo<'info>,

    // TODO: maybe remove IBC APP for sequence and check if it won't collide
    /// IBC client account
    /// CHECK: Router program validates this
    #[account()]
    pub ibc_client: AccountInfo<'info>,

    /// Pending transfer account - manually created with runtime-calculated sequence
    /// CHECK: Manually validated and created in instruction handler
    #[account(mut)]
    pub pending_transfer: UncheckedAccount<'info>,
}

// TODO: validate client id non zero/less than max
pub fn ift_transfer(ctx: Context<IFTTransfer>, msg: IFTTransferMsg) -> Result<u64> {
    let clock = Clock::get()?;

    require!(msg.amount > 0, IFTError::ZeroAmount);
    require!(!msg.receiver.is_empty(), IFTError::EmptyReceiver);
    require!(
        msg.receiver.len() <= MAX_RECEIVER_LENGTH,
        IFTError::InvalidReceiver
    );

    let timeout = if msg.timeout_timestamp == 0 {
        clock.unix_timestamp + DEFAULT_TIMEOUT_DURATION
    } else {
        require!(
            msg.timeout_timestamp > clock.unix_timestamp,
            IFTError::TimeoutInPast
        );
        require!(
            msg.timeout_timestamp <= clock.unix_timestamp + MAX_TIMEOUT_DURATION,
            IFTError::TimeoutTooLong
        );
        msg.timeout_timestamp
    };

    let burn_accounts = Burn {
        mint: ctx.accounts.mint.to_account_info(),
        from: ctx.accounts.sender_token_account.to_account_info(),
        authority: ctx.accounts.sender.to_account_info(),
    };
    let burn_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), burn_accounts);
    token_interface::burn(burn_ctx, msg.amount)?;
    ctx.accounts.mint.reload()?;
    ctx.accounts.sender_token_account.reload()?;

    crate::helpers::reduce_mint_rate_limit_usage(&mut ctx.accounts.app_state, msg.amount, &clock);

    let mint_call_payload = construct_mint_call(
        &ctx.accounts.ift_bridge.chain_options,
        &msg.receiver,
        msg.amount,
    )?;

    let gmp_accounts = SendGmpCallAccounts {
        gmp_program: ctx.accounts.gmp_program.clone(),
        gmp_app_state: ctx.accounts.gmp_app_state.clone(),
        sender: ctx.accounts.sender.to_account_info(),
        payer: ctx.accounts.payer.to_account_info(),
        router_program: ctx.accounts.router_program.to_account_info(),
        router_state: ctx.accounts.router_state.clone(),
        client_sequence: ctx.accounts.client_sequence.clone(),
        packet_commitment: ctx.accounts.packet_commitment.clone(),
        instruction_sysvar: ctx.accounts.instruction_sysvar.clone(),
        ibc_app: ctx.accounts.gmp_ibc_app.clone(),
        client: ctx.accounts.ibc_client.clone(),
        system_program: ctx.accounts.system_program.to_account_info(),
    };

    let gmp_msg = SendGmpCallMsg {
        source_client: msg.client_id.clone(),
        timeout_timestamp: timeout,
        receiver: ctx.accounts.ift_bridge.counterparty_ift_address.clone(),
        payload: mint_call_payload,
    };

    let sequence = crate::gmp_cpi::send_gmp_call(gmp_accounts, gmp_msg)?;

    create_pending_transfer_account(CreatePendingTransferParams {
        mint: &ctx.accounts.app_state.mint,
        client_id: &msg.client_id,
        sequence,
        sender: &ctx.accounts.sender.key(),
        amount: msg.amount,
        pending_transfer: &ctx.accounts.pending_transfer,
        payer: &ctx.accounts.payer.to_account_info(),
        system_program: &ctx.accounts.system_program.to_account_info(),
        clock: &clock,
    })?;

    emit!(IFTTransferInitiated {
        mint: ctx.accounts.app_state.mint,
        client_id: msg.client_id.clone(),
        sequence,
        sender: ctx.accounts.sender.key(),
        receiver: msg.receiver,
        amount: msg.amount,
        timeout_timestamp: timeout,
    });

    Ok(sequence)
}

/// Construct chain-specific mint call payload for the counterparty.
fn construct_mint_call(
    chain_options: &ChainOptions,
    receiver: &str,
    amount: u64,
) -> Result<Vec<u8>> {
    match chain_options {
        ChainOptions::Evm => construct_evm_mint_call(receiver, amount),
        ChainOptions::Cosmos {
            denom,
            type_url,
            ica_address,
        } => Ok(construct_cosmos_mint_call(
            type_url,
            ica_address,
            denom,
            receiver,
            amount,
        )),
    }
}

/// Construct ABI-encoded call to `iftMint(address, uint256)` for EVM chains.
pub fn encode_ift_mint_call(receiver: [u8; 20], amount: u64) -> Vec<u8> {
    use alloy_sol_types::private::{Address, U256};

    IFT::iftMintCall {
        receiver: Address::from(receiver),
        amount: U256::from(amount),
    }
    .abi_encode()
}

/// Construct ABI-encoded call to iftMint(address, uint256) for EVM chains.
fn construct_evm_mint_call(receiver: &str, amount: u64) -> Result<Vec<u8>> {
    let receiver_hex = receiver.trim_start_matches("0x");
    let receiver_bytes =
        hex::decode(receiver_hex).map_err(|_| error!(IFTError::InvalidReceiver))?;

    // lagging,, lagging
    // TODO: eth address const
    let receiver_array: [u8; 20] = receiver_bytes
        .try_into()
        .map_err(|_| error!(IFTError::InvalidReceiver))?;

    Ok(encode_ift_mint_call(receiver_array, amount))
}

// Using ABI JSON because sol! macro can't resolve Solidity imports.
alloy_sol_types::sol!(IFT, "../../../../abi/IFTOwnable.json");

/// Protojson representation of `MsgIFTMint` for Cosmos chains
#[derive(Serialize)]
struct MsgIFTMint<'a> {
    #[serde(rename = "@type")]
    type_url: &'a str,
    signer: &'a str,
    denom: &'a str,
    receiver: &'a str,
    amount: String,
}

/// Protojson representation of `CosmosTx` wrapper
#[derive(Serialize)]
struct CosmosTx<'a> {
    messages: Vec<MsgIFTMint<'a>>,
}

/// Construct protojson-encoded `CosmosTx` with `MsgIFTMint` for Cosmos chains
fn construct_cosmos_mint_call(
    type_url: &str,
    signer: &str,
    denom: &str,
    receiver: &str,
    amount: u64,
) -> Vec<u8> {
    // The signer is the ICS27-GMP interchain account on the Cosmos chain that controls the IFT mint
    let tx = CosmosTx {
        messages: vec![MsgIFTMint {
            type_url,
            signer,
            denom,
            receiver,
            amount: amount.to_string(),
        }],
    };
    // TODO: use proto
    serde_json::to_vec(&tx).expect("cannot fail for this simple struct")
}

/// Parameters for creating a pending transfer account
struct CreatePendingTransferParams<'a, 'info> {
    mint: &'a Pubkey,
    client_id: &'a str,
    sequence: u64,
    sender: &'a Pubkey,
    amount: u64,
    pending_transfer: &'a UncheckedAccount<'info>,
    payer: &'a AccountInfo<'info>,
    system_program: &'a AccountInfo<'info>,
    clock: &'a Clock,
}

/// Creates pending transfer PDA (sequence is runtime-computed, can't use Anchor's `init`)
fn create_pending_transfer_account(params: CreatePendingTransferParams) -> Result<()> {
    let CreatePendingTransferParams {
        mint,
        client_id,
        sequence,
        sender,
        amount,
        pending_transfer: pending_transfer_info,
        payer,
        system_program,
        clock,
    } = params;
    let sequence_bytes = sequence.to_le_bytes();

    // TODO: `find_program_address` is O(n) ~10k CUs. Consider accepting bump as parameter
    // (client computes off-chain via simulation) and using `create_program_address` ~1.5k CUs.
    let (expected_pda, bump) = Pubkey::find_program_address(
        &[
            PENDING_TRANSFER_SEED,
            mint.as_ref(),
            client_id.as_bytes(),
            &sequence_bytes,
        ],
        &crate::ID,
    );
    require!(
        pending_transfer_info.key() == expected_pda,
        IFTError::InvalidPendingTransfer
    );

    let account_size = 8 + PendingTransfer::INIT_SPACE;
    let lamports = Rent::get()?.minimum_balance(account_size);

    let signer_seeds: &[&[&[u8]]] = &[&[
        PENDING_TRANSFER_SEED,
        mint.as_ref(),
        client_id.as_bytes(),
        &sequence_bytes,
        &[bump],
    ]];

    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            pending_transfer_info.key,
            lamports,
            account_size as u64,
            &crate::ID,
        ),
        &[
            payer.clone(),
            pending_transfer_info.to_account_info(),
            system_program.clone(),
        ],
        signer_seeds,
    )?;

    let pending = PendingTransfer {
        version: AccountVersion::V1,
        bump,
        mint: *mint,
        client_id: client_id.to_string(),
        sequence,
        sender: *sender,
        amount,
        timestamp: clock.unix_timestamp,
        _reserved: [0; 32],
    };

    let mut data = pending_transfer_info.try_borrow_mut_data()?;
    data[0..8].copy_from_slice(PendingTransfer::DISCRIMINATOR);
    pending.serialize(&mut &mut data[8..])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::IFTTransferMsg;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use rstest::rstest;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    const TEST_CLIENT_ID: &str = "07-tendermint-0";
    const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";
    const VALID_RECEIVER: &str = "0xabcdef1234567890abcdef1234567890abcdef12";

    #[rstest]
    #[case::invalid_hex("0xnothex")]
    #[case::short_address("0xabcd")]
    #[case::empty_address("")]
    #[case::only_prefix("0x")]
    fn test_construct_evm_mint_call_invalid_receiver(#[case] invalid_receiver: &str) {
        assert!(construct_evm_mint_call(invalid_receiver, 100).is_err());
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

    #[derive(Clone)]
    struct MintCallTestCase {
        chain_options: ChainOptions,
        receiver: &'static str,
        expected_len: Option<usize>,
        expected_content: Vec<&'static str>,
    }

    fn evm_test_case() -> MintCallTestCase {
        MintCallTestCase {
            chain_options: ChainOptions::Evm,
            receiver: "0x1234567890abcdef1234567890abcdef12345678",
            expected_len: Some(68),
            expected_content: vec![],
        }
    }

    fn cosmos_test_case() -> MintCallTestCase {
        MintCallTestCase {
            chain_options: ChainOptions::Cosmos {
                denom: "uatom".to_string(),
                type_url: "/cosmos.ift.v1.MsgIFTMint".to_string(),
                ica_address: "cosmos1icaaddress".to_string(),
            },
            receiver: "cosmos1receiver",
            expected_len: None,
            expected_content: vec!["/cosmos.ift.v1.MsgIFTMint", "uatom", "cosmos1icaaddress"],
        }
    }

    #[rstest]
    #[case::evm(evm_test_case())]
    #[case::cosmos(cosmos_test_case())]
    fn test_construct_mint_call(#[case] test_case: MintCallTestCase) {
        let result = construct_mint_call(&test_case.chain_options, test_case.receiver, 100);
        assert!(result.is_ok());
        let payload = result.unwrap();

        if let Some(expected_len) = test_case.expected_len {
            assert_eq!(payload.len(), expected_len);
        }

        if !test_case.expected_content.is_empty() {
            let content = String::from_utf8(payload).unwrap();
            for expected in test_case.expected_content {
                assert!(
                    content.contains(expected),
                    "Expected to contain: {expected}"
                );
            }
        }
    }

    fn create_token_account(
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
    ) -> solana_sdk::account::Account {
        let mut data = vec![0u8; 165];
        data[0..32].copy_from_slice(&mint.to_bytes());
        data[32..64].copy_from_slice(&owner.to_bytes());
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = 1;

        solana_sdk::account::Account {
            lamports: 1_000_000,
            data,
            owner: anchor_spl::token::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_mint_account(mint_authority: Option<&Pubkey>) -> solana_sdk::account::Account {
        let mut data = vec![0u8; 82];
        if let Some(authority) = mint_authority {
            data[0..4].copy_from_slice(&1u32.to_le_bytes());
            data[4..36].copy_from_slice(&authority.to_bytes());
        }
        data[44] = 9;
        data[45] = 1;

        solana_sdk::account::Account {
            lamports: 1_000_000,
            data,
            owner: anchor_spl::token::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

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
        TokenPaused,
    }

    #[allow(clippy::struct_excessive_bools)]
    struct TransferTestConfig {
        bridge_active: bool,
        amount: u64,
        receiver: String,
        sender_is_signer: bool,
        use_wrong_token_owner: bool,
        use_wrong_token_mint: bool,
        timeout_timestamp: i64,
        token_paused: bool,
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
                timeout_timestamp: 0,
                token_paused: false,
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
                    timeout_timestamp: 1,
                    ..default
                },
                TransferErrorCase::TimeoutTooLong => Self {
                    timeout_timestamp: i64::MAX,
                    ..default
                },
                TransferErrorCase::ReceiverTooLong => Self {
                    receiver: "x".repeat(crate::constants::MAX_RECEIVER_LENGTH + 1),
                    ..default
                },
                TransferErrorCase::TokenPaused => Self {
                    token_paused: true,
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

        let app_state_account = create_ift_app_state_account_with_options(
            mint,
            app_state_bump,
            mint_authority_bump,
            Pubkey::new_unique(),
            gmp_program,
            config.token_paused,
        );

        let ift_bridge_account = create_ift_bridge_account(
            mint,
            TEST_CLIENT_ID,
            TEST_COUNTERPARTY_ADDRESS,
            ChainOptions::Evm,
            ift_bridge_bump,
            config.bridge_active,
        );

        let mint_account = create_mint_account(None);
        let sender_token_pda = Pubkey::new_unique();

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

    #[rstest]
    #[case::inactive_bridge(TransferErrorCase::InactiveBridge)]
    #[case::zero_amount(TransferErrorCase::ZeroAmount)]
    #[case::empty_receiver(TransferErrorCase::EmptyReceiver)]
    #[case::sender_not_signer(TransferErrorCase::SenderNotSigner)]
    #[case::wrong_token_account_owner(TransferErrorCase::WrongTokenAccountOwner)]
    #[case::wrong_token_mint(TransferErrorCase::WrongTokenMint)]
    #[case::timeout_in_past(TransferErrorCase::TimeoutInPast)]
    #[case::timeout_too_long(TransferErrorCase::TimeoutTooLong)]
    #[case::receiver_too_long(TransferErrorCase::ReceiverTooLong)]
    #[case::token_paused(TransferErrorCase::TokenPaused)]
    fn test_ift_transfer_validation(#[case] case: TransferErrorCase) {
        run_transfer_error_test(case);
    }
}
