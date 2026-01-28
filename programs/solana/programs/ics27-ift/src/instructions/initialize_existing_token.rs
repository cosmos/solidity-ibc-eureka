use anchor_lang::prelude::*;
use anchor_spl::token::spl_token::instruction::AuthorityType;
use anchor_spl::token::{set_authority, Mint, SetAuthority, Token};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::ExistingTokenInitialized;
use crate::state::{AccountVersion, IFTAppState};

#[derive(Accounts)]
pub struct InitializeExistingToken<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + IFTAppState::INIT_SPACE,
        seeds = [IFT_APP_STATE_SEED, mint.key().as_ref()],
        bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    #[account(
        mut,
        constraint = mint.mint_authority.is_some() @ IFTError::MintAuthorityNotSet
    )]
    pub mint: Account<'info, Mint>,

    /// CHECK: PDA that will become the new mint authority
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump
    )]
    pub mint_authority: AccountInfo<'info>,

    #[account(
        constraint = mint.mint_authority.unwrap() == current_authority.key() @ IFTError::InvalidMintAuthority
    )]
    pub current_authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn initialize_existing_token(
    ctx: Context<InitializeExistingToken>,
    access_manager: Pubkey,
    gmp_program: Pubkey,
) -> Result<()> {
    let cpi_accounts = SetAuthority {
        account_or_mint: ctx.accounts.mint.to_account_info(),
        current_authority: ctx.accounts.current_authority.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    set_authority(
        cpi_ctx,
        AuthorityType::MintTokens,
        Some(ctx.accounts.mint_authority.key()),
    )?;

    let app_state = &mut ctx.accounts.app_state;
    app_state.version = AccountVersion::V1;
    app_state.bump = ctx.bumps.app_state;
    app_state.mint = ctx.accounts.mint.key();
    app_state.mint_authority_bump = ctx.bumps.mint_authority;
    app_state.access_manager = access_manager;
    app_state.gmp_program = gmp_program;

    let clock = Clock::get()?;
    emit!(ExistingTokenInitialized {
        mint: ctx.accounts.mint.key(),
        decimals: ctx.accounts.mint.decimals,
        previous_authority: ctx.accounts.current_authority.key(),
        access_manager,
        gmp_program,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    use crate::test_utils::*;

    fn create_mint_account_no_authority(decimals: u8) -> solana_sdk::account::Account {
        use anchor_spl::token::spl_token;
        use solana_sdk::program_pack::Pack;

        let mint = spl_token::state::Mint {
            mint_authority: solana_sdk::program_option::COption::None,
            supply: 1_000_000_000,
            decimals,
            is_initialized: true,
            freeze_authority: solana_sdk::program_option::COption::None,
        };

        let mut data = vec![0u8; spl_token::state::Mint::LEN];
        mint.pack_into_slice(&mut data);

        solana_sdk::account::Account {
            lamports: 1_000_000,
            data,
            owner: spl_token::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    struct TestContext {
        mollusk: mollusk_svm::Mollusk,
        mint: Pubkey,
        current_authority: Pubkey,
        payer: Pubkey,
        app_state_pda: Pubkey,
        mint_authority_pda: Pubkey,
        access_manager: Pubkey,
        gmp_program: Pubkey,
    }

    fn setup_test() -> TestContext {
        let mollusk = setup_mollusk();
        let mint = Pubkey::new_unique();
        let current_authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (app_state_pda, _) = get_app_state_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let access_manager = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        TestContext {
            mollusk,
            mint,
            current_authority,
            payer,
            app_state_pda,
            mint_authority_pda,
            access_manager,
            gmp_program,
        }
    }

    fn build_instruction(ctx: &TestContext) -> Instruction {
        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(ctx.app_state_pda, false),
                AccountMeta::new(ctx.mint, false),
                AccountMeta::new_readonly(ctx.mint_authority_pda, false),
                AccountMeta::new_readonly(ctx.current_authority, true),
                AccountMeta::new(ctx.payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: crate::instruction::InitializeExistingToken {
                access_manager: ctx.access_manager,
                gmp_program: ctx.gmp_program,
            }
            .data(),
        }
    }

    #[test]
    fn test_mint_without_authority_fails() {
        let ctx = setup_test();
        let instruction = build_instruction(&ctx);

        let (token_program, token_program_account) = create_token_program_account();
        let (system_program, system_program_account) = create_system_program_account();
        let (clock_sysvar, clock_account) = create_clock_sysvar_account(1_700_000_000);

        let accounts = vec![
            (ctx.app_state_pda, create_uninitialized_pda()),
            (ctx.mint, create_mint_account_no_authority(6)),
            (
                ctx.mint_authority_pda,
                solana_sdk::account::Account {
                    lamports: 0,
                    data: vec![],
                    owner: solana_sdk::system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (ctx.current_authority, create_signer_account()),
            (ctx.payer, create_signer_account()),
            (token_program, token_program_account),
            (system_program, system_program_account),
            (clock_sysvar, clock_account),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_wrong_authority_signer_fails() {
        let ctx = setup_test();
        let instruction = build_instruction(&ctx);

        let actual_authority = Pubkey::new_unique();
        let (token_program, token_program_account) = create_token_program_account();
        let (system_program, system_program_account) = create_system_program_account();
        let (clock_sysvar, clock_account) = create_clock_sysvar_account(1_700_000_000);

        let accounts = vec![
            (ctx.app_state_pda, create_uninitialized_pda()),
            (ctx.mint, create_mint_account(actual_authority, 6)), // Different authority
            (
                ctx.mint_authority_pda,
                solana_sdk::account::Account {
                    lamports: 0,
                    data: vec![],
                    owner: solana_sdk::system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (ctx.current_authority, create_signer_account()), // Signs but doesn't match mint authority
            (ctx.payer, create_signer_account()),
            (token_program, token_program_account),
            (system_program, system_program_account),
            (clock_sysvar, clock_account),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_app_state_already_exists_fails() {
        let ctx = setup_test();
        let instruction = build_instruction(&ctx);

        let (token_program, token_program_account) = create_token_program_account();
        let (system_program, system_program_account) = create_system_program_account();
        let (clock_sysvar, clock_account) = create_clock_sysvar_account(1_700_000_000);
        let (_, app_state_bump) = get_app_state_pda(&ctx.mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&ctx.mint);

        let accounts = vec![
            (
                ctx.app_state_pda,
                create_ift_app_state_account(
                    ctx.mint,
                    app_state_bump,
                    mint_authority_bump,
                    ctx.access_manager,
                    ctx.gmp_program,
                ),
            ),
            (ctx.mint, create_mint_account(ctx.current_authority, 6)),
            (
                ctx.mint_authority_pda,
                solana_sdk::account::Account {
                    lamports: 0,
                    data: vec![],
                    owner: solana_sdk::system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (ctx.current_authority, create_signer_account()),
            (ctx.payer, create_signer_account()),
            (token_program, token_program_account),
            (system_program, system_program_account),
            (clock_sysvar, clock_account),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }
}
