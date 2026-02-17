//! Finalize transfer instruction for IFT
//!
//! This instruction allows anyone to finalize a pending transfer
//! after the GMP result has been recorded (either ack or timeout).

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::get_stack_height;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use solana_ibc_types::{CallResultStatus, GMPCallResult};

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
        seeds::program = app_state.gmp_program,
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
}

/// Finalize a pending transfer based on GMP result
pub fn finalize_transfer(
    ctx: Context<FinalizeTransfer>,
    client_id: String,
    sequence: u64,
) -> Result<()> {
    require!(get_stack_height() <= 1, IFTError::CpiNotAllowed);

    let pending = &ctx.accounts.pending_transfer;
    let gmp_result = &ctx.accounts.gmp_result;
    let clock = Clock::get()?;

    // Verify the GMP result matches expectations
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
        pubkey::Pubkey,
    };

    use crate::evm_selectors::ERROR_ACK_COMMITMENT;
    use crate::state::ChainOptions;
    use crate::test_utils::*;

    const TEST_CLIENT_ID: &str = "07-tendermint-0";
    const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";
    const TEST_SEQUENCE: u64 = 42;
    const TEST_AMOUNT: u64 = 1_000_000;

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

    struct FinalizeTransferTestSetup {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
    }

    fn build_finalize_transfer_test_setup(
        status: CallResultStatus,
        gmp_result_sender: Pubkey,
        gmp_result_client_id: &str,
        gmp_result_sequence: u64,
    ) -> FinalizeTransferTestSetup {
        let mint = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (pending_transfer_pda, pending_transfer_bump) =
            get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
        let (gmp_result_pda, gmp_result_bump) =
            get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE, &gmp_program);
        let (system_program, system_account) = create_system_program_account();

        let app_state_account =
            create_ift_app_state_account(app_state_bump, Pubkey::new_unique(), gmp_program);

        let app_mint_state_account =
            create_ift_app_mint_state_account(mint, app_mint_state_bump, mint_authority_bump);

        let ift_bridge_account = create_ift_bridge_account(
            mint,
            TEST_CLIENT_ID,
            TEST_COUNTERPARTY_ADDRESS,
            crate::state::ChainOptions::Evm,
            ift_bridge_bump,
            true,
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
            &gmp_program,
        );

        let mint_account = create_mint_account(Some(&mint_authority_pda));

        let mint_authority_account = solana_sdk::account::Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let sender_token_pda = Pubkey::new_unique();
        let sender_token_account = create_token_account(&mint, &sender, 0);

        let token_program_account = solana_sdk::account::Account {
            lamports: 1,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        };

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
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
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
            (mint_authority_pda, mint_authority_account),
            (sender_token_pda, sender_token_account),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        FinalizeTransferTestSetup {
            instruction,
            accounts,
        }
    }

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

    #[test]
    fn test_finalize_transfer_wrong_gmp_sender_fails() {
        let mollusk = setup_mollusk();

        let wrong_sender = Pubkey::new_unique();
        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Timeout,
            wrong_sender,
            TEST_CLIENT_ID,
            TEST_SEQUENCE,
        );

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_finalize_transfer_wrong_client_id_fails() {
        let mollusk = setup_mollusk();

        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Timeout,
            crate::ID,
            "wrong-client-id",
            TEST_SEQUENCE,
        );

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_finalize_transfer_wrong_sequence_fails() {
        let mollusk = setup_mollusk();

        let setup = build_finalize_transfer_test_setup(
            CallResultStatus::Timeout,
            crate::ID,
            TEST_CLIENT_ID,
            999,
        );

        let result = mollusk.process_instruction(&setup.instruction, &setup.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_finalize_transfer_token_account_wrong_owner_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let wrong_owner = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (pending_transfer_pda, pending_transfer_bump) =
            get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
        let (gmp_result_pda, gmp_result_bump) =
            get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE, &gmp_program);
        let (system_program, system_account) = create_system_program_account();

        let app_state_account =
            create_ift_app_state_account(app_state_bump, Pubkey::new_unique(), gmp_program);

        let app_mint_state_account =
            create_ift_app_mint_state_account(mint, app_mint_state_bump, mint_authority_bump);

        let ift_bridge_account = create_ift_bridge_account(
            mint,
            TEST_CLIENT_ID,
            TEST_COUNTERPARTY_ADDRESS,
            ChainOptions::Evm,
            ift_bridge_bump,
            true,
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
            crate::ID,
            TEST_SEQUENCE,
            TEST_CLIENT_ID,
            "dest-client",
            CallResultStatus::Timeout,
            gmp_result_bump,
            &gmp_program,
        );

        let mint_account = create_mint_account(Some(&mint_authority_pda));

        let mint_authority_account = solana_sdk::account::Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let sender_token_pda = Pubkey::new_unique();
        let sender_token_account = create_token_account(&mint, &wrong_owner, 0);

        let token_program_account = solana_sdk::account::Account {
            lamports: 1,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        };

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
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
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
            (mint_authority_pda, mint_authority_account),
            (sender_token_pda, sender_token_account),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "finalize_transfer should fail when token account owner doesn't match pending transfer sender"
        );
    }

    #[test]
    fn test_finalize_transfer_token_account_wrong_mint_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let wrong_mint = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (pending_transfer_pda, pending_transfer_bump) =
            get_pending_transfer_pda(&mint, TEST_CLIENT_ID, TEST_SEQUENCE);
        let (gmp_result_pda, gmp_result_bump) =
            get_gmp_result_pda(TEST_CLIENT_ID, TEST_SEQUENCE, &gmp_program);
        let (system_program, system_account) = create_system_program_account();

        let app_state_account =
            create_ift_app_state_account(app_state_bump, Pubkey::new_unique(), gmp_program);

        let app_mint_state_account =
            create_ift_app_mint_state_account(mint, app_mint_state_bump, mint_authority_bump);

        let ift_bridge_account = create_ift_bridge_account(
            mint,
            TEST_CLIENT_ID,
            TEST_COUNTERPARTY_ADDRESS,
            ChainOptions::Evm,
            ift_bridge_bump,
            true,
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
            crate::ID,
            TEST_SEQUENCE,
            TEST_CLIENT_ID,
            "dest-client",
            CallResultStatus::Timeout,
            gmp_result_bump,
            &gmp_program,
        );

        let mint_account = create_mint_account(Some(&mint_authority_pda));

        let mint_authority_account = solana_sdk::account::Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let sender_token_pda = Pubkey::new_unique();
        let sender_token_account = create_token_account(&wrong_mint, &sender, 0);

        let token_program_account = solana_sdk::account::Account {
            lamports: 1,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        };

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
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
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
            (mint_authority_pda, mint_authority_account),
            (sender_token_pda, sender_token_account),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "finalize_transfer should fail when token account mint doesn't match"
        );
    }
}
