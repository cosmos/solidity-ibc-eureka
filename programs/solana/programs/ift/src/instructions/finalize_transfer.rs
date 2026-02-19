//! Finalize transfer instruction for IFT
//!
//! This instruction allows anyone to finalize a pending transfer
//! after the GMP result has been recorded (either ack or timeout).

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use solana_ibc_types::{reject_cpi, CallResultStatus, GMPCallResult};

use crate::constants::{
    IFT_APP_MINT_STATE_SEED, IFT_APP_STATE_SEED, IFT_BRIDGE_SEED, MINT_AUTHORITY_SEED,
    PENDING_TRANSFER_SEED,
};
use crate::errors::IFTError;
use crate::events::{IFTTransferCompleted, IFTTransferRefunded, RefundReason};
use crate::evm_selectors::ERROR_ACK_COMMITMENT;
use crate::helpers::mint_to_account;
use crate::state::{IFTAppMintState, IFTAppState, IFTBridge, PendingTransfer};

/// Accounts for the `finalize_transfer` instruction
#[derive(Accounts)]
#[instruction(client_id: String, sequence: u64)]
pub struct FinalizeTransfer<'info> {
    /// Global IFT app state (read-only)
    #[account(
        seeds = [IFT_APP_STATE_SEED],
        bump = app_state.bump,
        constraint = !app_state.paused @ IFTError::AppPaused
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Per-mint IFT app state (mut for rate limit updates)
    #[account(
        mut,
        seeds = [IFT_APP_MINT_STATE_SEED, mint.key().as_ref()],
        bump = app_mint_state.bump
    )]
    pub app_mint_state: Account<'info, IFTAppMintState>,

    /// IFT bridge for this client
    #[account(
        seeds = [IFT_BRIDGE_SEED, app_mint_state.mint.as_ref(), client_id.as_bytes()],
        bump = ift_bridge.bump,
        constraint = ift_bridge.mint == app_mint_state.mint @ IFTError::InvalidBridge,
        constraint = ift_bridge.active @ IFTError::BridgeNotActive,
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// Pending transfer to process
    #[account(
        mut,
        close = payer,
        seeds = [
            PENDING_TRANSFER_SEED,
            app_mint_state.mint.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes()
        ],
        bump = pending_transfer.bump,
        constraint = pending_transfer.mint == app_mint_state.mint @ IFTError::PendingTransferNotFound,
        constraint = pending_transfer.client_id == client_id @ IFTError::GmpResultClientMismatch,
        constraint = pending_transfer.sequence == sequence @ IFTError::GmpResultSequenceMismatch,
    )]
    pub pending_transfer: Account<'info, PendingTransfer>,

    /// GMP result account - proves the ack/timeout happened
    /// This is a cross-program account owned by the GMP program
    #[account(
        seeds = [GMPCallResult::SEED, client_id.as_bytes(), &sequence.to_le_bytes()],
        seeds::program = ics27_gmp::ID,
        bump,
    )]
    pub gmp_result: Account<'info, ics27_gmp::state::GMPCallResultAccount>,

    /// SPL Token mint
    #[account(mut, address = app_mint_state.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA verified by seeds constraint
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump = app_mint_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// Original sender's token account (for refunds)
    #[account(
        mut,
        constraint = sender_token_account.mint == mint.key() @ IFTError::TokenAccountOwnerMismatch,
        constraint = sender_token_account.owner == pending_transfer.sender @ IFTError::TokenAccountOwnerMismatch
    )]
    pub sender_token_account: InterfaceAccount<'info, TokenAccount>,

    /// Payer receives rent from closed `PendingTransfer` account
    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

/// Finalize a pending transfer based on GMP result
pub fn finalize_transfer(
    ctx: Context<FinalizeTransfer>,
    client_id: String,
    sequence: u64,
) -> Result<()> {
    reject_cpi(&ctx.accounts.instructions_sysvar, &crate::ID).map_err(IFTError::from)?;

    let pending = &ctx.accounts.pending_transfer;
    let gmp_result = &ctx.accounts.gmp_result;
    let clock = Clock::get()?;

    // Verify the GMP result was initiated by this IFT program.
    // GMP's `send_call_cpi` extracts the calling program ID as the sender.
    require!(
        gmp_result.sender == crate::ID,
        IFTError::GmpResultSenderMismatch
    );
    require!(
        gmp_result.source_client == client_id,
        IFTError::GmpResultClientMismatch
    );
    require!(
        gmp_result.sequence == sequence,
        IFTError::GmpResultSequenceMismatch
    );

    match gmp_result.status {
        CallResultStatus::Timeout => {
            mint_to_account(
                &ctx.accounts.mint,
                &ctx.accounts.sender_token_account,
                &ctx.accounts.mint_authority,
                ctx.accounts.app_mint_state.mint_authority_bump,
                &ctx.accounts.token_program,
                pending.amount,
            )?;
            ctx.accounts.mint.reload()?;
            ctx.accounts.sender_token_account.reload()?;

            emit!(IFTTransferRefunded {
                mint: ctx.accounts.app_mint_state.mint,
                client_id: pending.client_id.clone(),
                sequence: pending.sequence,
                sender: pending.sender,
                amount: pending.amount,
                reason: RefundReason::Timeout,
                timestamp: clock.unix_timestamp,
            });
        }
        CallResultStatus::Acknowledgement(commitment) => {
            if commitment == ERROR_ACK_COMMITMENT {
                mint_to_account(
                    &ctx.accounts.mint,
                    &ctx.accounts.sender_token_account,
                    &ctx.accounts.mint_authority,
                    ctx.accounts.app_mint_state.mint_authority_bump,
                    &ctx.accounts.token_program,
                    pending.amount,
                )?;

                emit!(IFTTransferRefunded {
                    mint: ctx.accounts.app_mint_state.mint,
                    client_id: pending.client_id.clone(),
                    sequence: pending.sequence,
                    sender: pending.sender,
                    amount: pending.amount,
                    reason: RefundReason::Failed,
                    timestamp: clock.unix_timestamp,
                });
            } else {
                crate::helpers::reduce_mint_rate_limit_usage(
                    &mut ctx.accounts.app_mint_state,
                    pending.amount,
                    &clock,
                );

                emit!(IFTTransferCompleted {
                    mint: ctx.accounts.app_mint_state.mint,
                    client_id: pending.client_id.clone(),
                    sequence: pending.sequence,
                    sender: pending.sender,
                    amount: pending.amount,
                    timestamp: clock.unix_timestamp,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::InstructionData;
    use solana_ibc_types::{packet_acknowledgement_commitment_bytes32, CallResultStatus};
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        program_pack::Pack,
        pubkey::Pubkey,
    };

    use crate::errors::IFTError;
    use crate::evm_selectors::ERROR_ACK_COMMITMENT;
    use crate::state::ChainOptions;
    use crate::test_utils::*;

    const TEST_CLIENT_ID: &str = "07-tendermint-0";
    const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";
    const TEST_SEQUENCE: u64 = 42;
    const TEST_AMOUNT: u64 = 1_000_000;
    const TOKEN_DECIMALS: u8 = 9;

    // Account indices in instruction's account list
    const APP_MINT_STATE_IDX: usize = 1;
    const PENDING_TRANSFER_IDX: usize = 3;
    const MINT_IDX: usize = 5;
    const SENDER_TOKEN_IDX: usize = 7;

    fn empty_pda_account() -> solana_sdk::account::Account {
        solana_sdk::account::Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    struct FinalizeTransferTestSetup {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
    }

    struct FinalizeTransferTestSetupParams<'a> {
        status: CallResultStatus,
        gmp_result_sender: Option<Pubkey>,
        gmp_result_client_id: &'a str,
        gmp_result_sequence: u64,
        app_mint_state_override: Option<IftAppMintStateParams>,
        token_owner_override: Option<Pubkey>,
        token_mint_override: Option<Pubkey>,
        bridge_active: bool,
        app_paused: bool,
    }

    fn build_finalize_transfer_test_setup(
        status: CallResultStatus,
        gmp_result_sender: Option<Pubkey>,
        gmp_result_client_id: &str,
        gmp_result_sequence: u64,
    ) -> FinalizeTransferTestSetup {
        build_finalize_transfer_test_setup_from_params(FinalizeTransferTestSetupParams {
            status,
            gmp_result_sender,
            gmp_result_client_id,
            gmp_result_sequence,
            app_mint_state_override: None,
            token_owner_override: None,
            token_mint_override: None,
            bridge_active: true,
            app_paused: false,
        })
    }

    fn build_finalize_transfer_test_setup_from_params(
        params: FinalizeTransferTestSetupParams<'_>,
    ) -> FinalizeTransferTestSetup {
        let FinalizeTransferTestSetupParams {
            status,
            gmp_result_sender,
            gmp_result_client_id,
            gmp_result_sequence,
            app_mint_state_override,
            token_owner_override,
            token_mint_override,
            bridge_active,
            app_paused,
        } = params;
        let mint = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let gmp_result_sender = gmp_result_sender.unwrap_or(crate::ID);
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (pending_transfer_pda, pending_transfer_bump) =
            get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
        let (gmp_result_pda, gmp_result_bump) = get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE);
        let (system_program, system_account) = create_system_program_account();
        let (token_program_id, token_program_account) = token_program_keyed_account();
        let (sysvar_id, sysvar_account) = create_instructions_sysvar_account();

        let app_state_account = create_ift_app_state_account_with_options(
            app_state_bump,
            Pubkey::new_unique(),
            app_paused,
        );

        let app_mint_state_account = app_mint_state_override.map_or_else(
            || create_ift_app_mint_state_account(mint, app_mint_state_bump, mint_authority_bump),
            |params| {
                create_ift_app_mint_state_account_full(IftAppMintStateParams {
                    mint,
                    bump: app_mint_state_bump,
                    mint_authority_bump,
                    ..params
                })
            },
        );

        let ift_bridge_account = create_ift_bridge_account(
            mint,
            TEST_CLIENT_ID,
            TEST_COUNTERPARTY_ADDRESS,
            ChainOptions::Evm,
            ift_bridge_bump,
            bridge_active,
        );

        let pending_transfer_account = create_pending_transfer_account(
            mint,
            TEST_CLIENT_ID,
            TEST_SEQUENCE,
            sender,
            TEST_AMOUNT,
            pending_transfer_bump,
        );

        let gmp_result_account = create_gmp_result_account(
            gmp_result_sender,
            gmp_result_sequence,
            gmp_result_client_id,
            "dest-client",
            status,
            gmp_result_bump,
        );

        let mint_account = create_mint_account(mint_authority_pda, TOKEN_DECIMALS);

        let token_account_mint = token_mint_override.unwrap_or(mint);
        let token_account_owner = token_owner_override.unwrap_or(sender);
        let sender_token_pda = Pubkey::new_unique();
        let sender_token_account = create_token_account(token_account_mint, token_account_owner, 0);

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(app_mint_state_pda, false),
                AccountMeta::new_readonly(ift_bridge_pda, false),
                AccountMeta::new(pending_transfer_pda, false),
                AccountMeta::new_readonly(gmp_result_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(sender_token_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(token_program_id, false),
                AccountMeta::new_readonly(system_program, false),
                AccountMeta::new_readonly(sysvar_id, false),
            ],
            data: crate::instruction::FinalizeTransfer {
                client_id: TEST_CLIENT_ID.to_string(),
                sequence: TEST_SEQUENCE,
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (app_mint_state_pda, app_mint_state_account),
            (ift_bridge_pda, ift_bridge_account),
            (pending_transfer_pda, pending_transfer_account),
            (gmp_result_pda, gmp_result_account),
            (mint, mint_account),
            (mint_authority_pda, empty_pda_account()),
            (sender_token_pda, sender_token_account),
            (payer, create_signer_account()),
            (token_program_id, token_program_account),
            (system_program, system_account),
            (sysvar_id, sysvar_account),
        ];

        FinalizeTransferTestSetup {
            instruction,
            accounts,
        }
    }

    fn unpack_token_balance(result: &mollusk_svm::result::InstructionResult, index: usize) -> u64 {
        let (_, account) = &result.resulting_accounts[index];
        anchor_spl::token::spl_token::state::Account::unpack(&account.data)
            .expect("valid token account")
            .amount
    }

    fn unpack_mint_supply(result: &mollusk_svm::result::InstructionResult, index: usize) -> u64 {
        let (_, account) = &result.resulting_accounts[index];
        anchor_spl::token::spl_token::state::Mint::unpack(&account.data)
            .expect("valid mint account")
            .supply
    }

    // ─── Constant tests ─────────────────────────────────────────────

    #[test]
    fn test_error_ack_commitment_matches_runtime_computation() {
        let error_ack = solana_ibc_types::UNIVERSAL_ERROR_ACK;
        let computed =
            packet_acknowledgement_commitment_bytes32(std::slice::from_ref(&error_ack.to_vec()))
                .expect("single ack is never empty");

        assert_eq!(
            ERROR_ACK_COMMITMENT, computed,
            "Precomputed ERROR_ACK_COMMITMENT must match runtime computation"
        );
    }

    #[test]
    fn test_error_ack_commitment_is_valid() {
        assert_eq!(ERROR_ACK_COMMITMENT.len(), 32);
        assert!(ERROR_ACK_COMMITMENT.iter().any(|&b| b != 0));
    }

    // ─── Validation failure tests ───────────────────────────────────

    fn assert_finalize_error(
        mollusk: &mollusk_svm::Mollusk,
        setup: &FinalizeTransferTestSetup,
        expected_error: u32,
    ) {
        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert_eq!(
            result.program_result,
            Err(solana_sdk::instruction::InstructionError::Custom(
                expected_error
            ))
            .into(),
        );
    }

    #[test]
    fn test_finalize_transfer_wrong_gmp_sender_fails() {
        let mollusk = setup_mollusk();

        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Timeout,
            Some(Pubkey::new_unique()),
            TEST_CLIENT_ID,
            TEST_SEQUENCE,
        );

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::GmpResultSenderMismatch as u32,
        );
    }

    #[test]
    fn test_finalize_transfer_wrong_client_id_fails() {
        let mollusk = setup_mollusk();

        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Timeout,
            None,
            "wrong-client-id",
            TEST_SEQUENCE,
        );

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::GmpResultClientMismatch as u32,
        );
    }

    #[test]
    fn test_finalize_transfer_wrong_sequence_fails() {
        let mollusk = setup_mollusk();

        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Timeout,
            None,
            TEST_CLIENT_ID,
            999,
        );

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::GmpResultSequenceMismatch as u32,
        );
    }

    #[test]
    fn test_finalize_transfer_token_account_wrong_owner_fails() {
        let mollusk = setup_mollusk();

        let setup =
            build_finalize_transfer_test_setup_from_params(FinalizeTransferTestSetupParams {
                status: CallResultStatus::Timeout,
                gmp_result_sender: None,
                gmp_result_client_id: TEST_CLIENT_ID,
                gmp_result_sequence: TEST_SEQUENCE,
                app_mint_state_override: None,
                token_owner_override: Some(Pubkey::new_unique()), // wrong owner
                token_mint_override: None,
                bridge_active: true,
                app_paused: false,
            });

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::TokenAccountOwnerMismatch as u32,
        );
    }

    #[test]
    fn test_finalize_transfer_token_account_wrong_mint_fails() {
        let mollusk = setup_mollusk();

        let setup =
            build_finalize_transfer_test_setup_from_params(FinalizeTransferTestSetupParams {
                status: CallResultStatus::Timeout,
                gmp_result_sender: None,
                gmp_result_client_id: TEST_CLIENT_ID,
                gmp_result_sequence: TEST_SEQUENCE,
                app_mint_state_override: None,
                token_owner_override: None,
                token_mint_override: Some(Pubkey::new_unique()), // wrong mint
                bridge_active: true,
                app_paused: false,
            });

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::TokenAccountOwnerMismatch as u32,
        );
    }

    #[test]
    fn test_finalize_transfer_bridge_mint_mismatch_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let wrong_mint = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (pending_transfer_pda, pending_transfer_bump) =
            get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
        let (gmp_result_pda, gmp_result_bump) = get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE);
        let (system_program, system_account) = create_system_program_account();
        let (token_program_id, token_program_account) = token_program_keyed_account();
        let (sysvar_id, sysvar_account) = create_instructions_sysvar_account();

        let sender_token_pda = Pubkey::new_unique();

        let setup = FinalizeTransferTestSetup {
            instruction: Instruction {
                program_id: crate::ID,
                accounts: vec![
                    AccountMeta::new_readonly(app_state_pda, false),
                    AccountMeta::new(app_mint_state_pda, false),
                    AccountMeta::new_readonly(ift_bridge_pda, false),
                    AccountMeta::new(pending_transfer_pda, false),
                    AccountMeta::new_readonly(gmp_result_pda, false),
                    AccountMeta::new(mint, false),
                    AccountMeta::new_readonly(mint_authority_pda, false),
                    AccountMeta::new(sender_token_pda, false),
                    AccountMeta::new(payer, true),
                    AccountMeta::new_readonly(token_program_id, false),
                    AccountMeta::new_readonly(system_program, false),
                    AccountMeta::new_readonly(sysvar_id, false),
                ],
                data: crate::instruction::FinalizeTransfer {
                    client_id: TEST_CLIENT_ID.to_string(),
                    sequence: TEST_SEQUENCE,
                }
                .data(),
            },
            accounts: vec![
                (
                    app_state_pda,
                    create_ift_app_state_account(app_state_bump, Pubkey::new_unique()),
                ),
                (
                    app_mint_state_pda,
                    create_ift_app_mint_state_account(
                        mint,
                        app_mint_state_bump,
                        mint_authority_bump,
                    ),
                ),
                (
                    ift_bridge_pda,
                    create_ift_bridge_account(
                        wrong_mint, // mint mismatch triggers InvalidBridge
                        TEST_CLIENT_ID,
                        TEST_COUNTERPARTY_ADDRESS,
                        ChainOptions::Evm,
                        ift_bridge_bump,
                        true,
                    ),
                ),
                (
                    pending_transfer_pda,
                    create_pending_transfer_account(
                        mint,
                        TEST_CLIENT_ID,
                        TEST_SEQUENCE,
                        sender,
                        TEST_AMOUNT,
                        pending_transfer_bump,
                    ),
                ),
                (
                    gmp_result_pda,
                    create_gmp_result_account(
                        crate::ID,
                        TEST_SEQUENCE,
                        TEST_CLIENT_ID,
                        "dest-client",
                        CallResultStatus::Timeout,
                        gmp_result_bump,
                    ),
                ),
                (
                    mint,
                    create_mint_account(mint_authority_pda, TOKEN_DECIMALS),
                ),
                (mint_authority_pda, empty_pda_account()),
                (sender_token_pda, create_token_account(mint, sender, 0)),
                (payer, create_signer_account()),
                (token_program_id, token_program_account),
                (system_program, system_account),
                (sysvar_id, sysvar_account),
            ],
        };

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::InvalidBridge as u32,
        );
    }

    #[test]
    fn test_finalize_transfer_bridge_not_active_fails() {
        let mollusk = setup_mollusk();

        let setup =
            build_finalize_transfer_test_setup_from_params(FinalizeTransferTestSetupParams {
                status: CallResultStatus::Timeout,
                gmp_result_sender: None,
                gmp_result_client_id: TEST_CLIENT_ID,
                gmp_result_sequence: TEST_SEQUENCE,
                app_mint_state_override: None,
                token_owner_override: None,
                token_mint_override: None,
                bridge_active: false,
                app_paused: false,
            });

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::BridgeNotActive as u32,
        );
    }

    #[test]
    fn test_finalize_transfer_app_paused_fails() {
        let mollusk = setup_mollusk();

        let setup =
            build_finalize_transfer_test_setup_from_params(FinalizeTransferTestSetupParams {
                status: CallResultStatus::Timeout,
                gmp_result_sender: None,
                gmp_result_client_id: TEST_CLIENT_ID,
                gmp_result_sequence: TEST_SEQUENCE,
                app_mint_state_override: None,
                token_owner_override: None,
                token_mint_override: None,
                bridge_active: true,
                app_paused: true,
            });

        assert_finalize_error(
            &mollusk,
            &setup,
            ANCHOR_ERROR_OFFSET + IFTError::AppPaused as u32,
        );
    }

    // ─── Happy path tests ───────────────────────────────────────────

    #[test]
    fn test_finalize_transfer_timeout_refund_succeeds() {
        let mollusk = setup_mollusk_with_token();

        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Timeout,
            None,
            TEST_CLIENT_ID,
            TEST_SEQUENCE,
        );

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(
            !result.program_result.is_err(),
            "timeout refund should succeed: {:?}",
            result.program_result
        );

        assert_eq!(
            unpack_token_balance(&result, SENDER_TOKEN_IDX),
            TEST_AMOUNT,
            "sender should receive refund"
        );
        assert_eq!(
            unpack_mint_supply(&result, MINT_IDX),
            1_000_000_000 + TEST_AMOUNT,
            "mint supply should increase by refund amount"
        );

        let (_, pending) = &result.resulting_accounts[PENDING_TRANSFER_IDX];
        assert_eq!(pending.lamports, 0, "pending transfer should be closed");
    }

    #[test]
    fn test_finalize_transfer_error_ack_refund_succeeds() {
        let mollusk = setup_mollusk_with_token();

        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Acknowledgement(ERROR_ACK_COMMITMENT),
            None,
            TEST_CLIENT_ID,
            TEST_SEQUENCE,
        );

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(
            !result.program_result.is_err(),
            "error ack refund should succeed: {:?}",
            result.program_result
        );

        assert_eq!(
            unpack_token_balance(&result, SENDER_TOKEN_IDX),
            TEST_AMOUNT,
            "sender should receive refund on error ack"
        );
        assert_eq!(
            unpack_mint_supply(&result, MINT_IDX),
            1_000_000_000 + TEST_AMOUNT,
            "mint supply should increase by refund amount"
        );

        let (_, pending) = &result.resulting_accounts[PENDING_TRANSFER_IDX];
        assert_eq!(pending.lamports, 0, "pending transfer should be closed");
    }

    #[test]
    fn test_finalize_transfer_success_ack_succeeds() {
        let mollusk = setup_mollusk_with_token();

        let success_commitment = [42u8; 32];
        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Acknowledgement(success_commitment),
            None,
            TEST_CLIENT_ID,
            TEST_SEQUENCE,
        );

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(
            !result.program_result.is_err(),
            "success ack should succeed: {:?}",
            result.program_result
        );

        assert_eq!(
            unpack_token_balance(&result, SENDER_TOKEN_IDX),
            0,
            "no tokens should be minted on success"
        );
        assert_eq!(
            unpack_mint_supply(&result, MINT_IDX),
            1_000_000_000,
            "mint supply should be unchanged on success"
        );

        let (_, pending) = &result.resulting_accounts[PENDING_TRANSFER_IDX];
        assert_eq!(pending.lamports, 0, "pending transfer should be closed");
    }

    #[test]
    fn test_finalize_transfer_success_ack_reduces_rate_limit() {
        const RATE_LIMIT_TIMESTAMP: i64 = 1_700_000_000;
        const RATE_LIMIT_DAY: u64 = RATE_LIMIT_TIMESTAMP as u64 / 86400;
        const DAILY_LIMIT: u64 = 10_000_000;
        const INITIAL_USAGE: u64 = 5_000_000;

        let mut mollusk = setup_mollusk_with_token();
        mollusk.sysvars.clock.unix_timestamp = RATE_LIMIT_TIMESTAMP;

        let success_commitment = [42u8; 32];
        let setup =
            build_finalize_transfer_test_setup_from_params(FinalizeTransferTestSetupParams {
                status: CallResultStatus::Acknowledgement(success_commitment),
                gmp_result_sender: None,
                gmp_result_client_id: TEST_CLIENT_ID,
                gmp_result_sequence: TEST_SEQUENCE,
                app_mint_state_override: Some(IftAppMintStateParams {
                    mint: Pubkey::default(),
                    bump: 0,
                    mint_authority_bump: 0,
                    daily_mint_limit: DAILY_LIMIT,
                    rate_limit_day: RATE_LIMIT_DAY,
                    rate_limit_daily_usage: INITIAL_USAGE,
                }),
                token_owner_override: None,
                token_mint_override: None,
                bridge_active: true,
                app_paused: false,
            });

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(
            !result.program_result.is_err(),
            "success ack with rate limit should succeed: {:?}",
            result.program_result
        );

        let (_, mint_state_account) = &result.resulting_accounts[APP_MINT_STATE_IDX];
        let mint_state: crate::state::IFTAppMintState =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &mint_state_account.data[..])
                .expect("valid IFTAppMintState");

        assert_eq!(
            mint_state.rate_limit_daily_usage,
            INITIAL_USAGE - TEST_AMOUNT,
            "rate limit usage should be reduced by transfer amount"
        );
        assert_eq!(mint_state.rate_limit_day, RATE_LIMIT_DAY);
    }

    #[test]
    fn test_finalize_transfer_timeout_refund_with_existing_balance() {
        const EXISTING_BALANCE: u64 = 500_000;

        let mollusk = setup_mollusk_with_token();
        let mint = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (pending_transfer_pda, pending_transfer_bump) =
            get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
        let (gmp_result_pda, gmp_result_bump) = get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE);
        let (system_program, system_account) = create_system_program_account();
        let (token_program_id, token_program_account) = token_program_keyed_account();
        let (sysvar_id, sysvar_account) = create_instructions_sysvar_account();

        let sender_token_pda = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(app_mint_state_pda, false),
                AccountMeta::new_readonly(ift_bridge_pda, false),
                AccountMeta::new(pending_transfer_pda, false),
                AccountMeta::new_readonly(gmp_result_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(sender_token_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(token_program_id, false),
                AccountMeta::new_readonly(system_program, false),
                AccountMeta::new_readonly(sysvar_id, false),
            ],
            data: crate::instruction::FinalizeTransfer {
                client_id: TEST_CLIENT_ID.to_string(),
                sequence: TEST_SEQUENCE,
            }
            .data(),
        };

        let accounts = vec![
            (
                app_state_pda,
                create_ift_app_state_account(app_state_bump, Pubkey::new_unique()),
            ),
            (
                app_mint_state_pda,
                create_ift_app_mint_state_account(mint, app_mint_state_bump, mint_authority_bump),
            ),
            (
                ift_bridge_pda,
                create_ift_bridge_account(
                    mint,
                    TEST_CLIENT_ID,
                    TEST_COUNTERPARTY_ADDRESS,
                    ChainOptions::Evm,
                    ift_bridge_bump,
                    true,
                ),
            ),
            (
                pending_transfer_pda,
                create_pending_transfer_account(
                    mint,
                    TEST_CLIENT_ID,
                    TEST_SEQUENCE,
                    sender,
                    TEST_AMOUNT,
                    pending_transfer_bump,
                ),
            ),
            (
                gmp_result_pda,
                create_gmp_result_account(
                    crate::ID,
                    TEST_SEQUENCE,
                    TEST_CLIENT_ID,
                    "dest-client",
                    CallResultStatus::Timeout,
                    gmp_result_bump,
                ),
            ),
            (
                mint,
                create_mint_account(mint_authority_pda, TOKEN_DECIMALS),
            ),
            (mint_authority_pda, empty_pda_account()),
            (
                sender_token_pda,
                create_token_account(mint, sender, EXISTING_BALANCE),
            ),
            (payer, create_signer_account()),
            (token_program_id, token_program_account),
            (system_program, system_account),
            (sysvar_id, sysvar_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "timeout refund with existing balance should succeed: {:?}",
            result.program_result
        );

        assert_eq!(
            unpack_token_balance(&result, SENDER_TOKEN_IDX),
            EXISTING_BALANCE + TEST_AMOUNT,
            "sender should receive refund on top of existing balance"
        );
    }

    #[test]
    fn test_finalize_transfer_success_ack_saturating_rate_limit_reduction() {
        const RATE_LIMIT_TIMESTAMP: i64 = 1_700_000_000;
        const RATE_LIMIT_DAY: u64 = RATE_LIMIT_TIMESTAMP as u64 / 86400;
        const DAILY_LIMIT: u64 = 10_000_000;
        // Usage less than TEST_AMOUNT — reduction should saturate to 0
        const INITIAL_USAGE: u64 = 500_000;

        let mut mollusk = setup_mollusk_with_token();
        mollusk.sysvars.clock.unix_timestamp = RATE_LIMIT_TIMESTAMP;

        let success_commitment = [42u8; 32];
        let setup =
            build_finalize_transfer_test_setup_from_params(FinalizeTransferTestSetupParams {
                status: CallResultStatus::Acknowledgement(success_commitment),
                gmp_result_sender: None,
                gmp_result_client_id: TEST_CLIENT_ID,
                gmp_result_sequence: TEST_SEQUENCE,
                app_mint_state_override: Some(IftAppMintStateParams {
                    mint: Pubkey::default(),
                    bump: 0,
                    mint_authority_bump: 0,
                    daily_mint_limit: DAILY_LIMIT,
                    rate_limit_day: RATE_LIMIT_DAY,
                    rate_limit_daily_usage: INITIAL_USAGE,
                }),
                token_owner_override: None,
                token_mint_override: None,
                bridge_active: true,
                app_paused: false,
            });

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(
            !result.program_result.is_err(),
            "success ack with saturating rate limit should succeed: {:?}",
            result.program_result
        );

        let (_, mint_state_account) = &result.resulting_accounts[APP_MINT_STATE_IDX];
        let mint_state: crate::state::IFTAppMintState =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &mint_state_account.data[..])
                .expect("valid IFTAppMintState");

        assert_eq!(
            mint_state.rate_limit_daily_usage, 0,
            "rate limit usage should saturate to 0 when amount > usage"
        );
    }
}
