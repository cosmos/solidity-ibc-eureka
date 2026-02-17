use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::{AdminMintExecuted, AdminUpdated, MintAuthorityRevoked};
use crate::helpers::{check_and_update_mint_rate_limit, mint_to_account};
use crate::state::{AdminMintMsg, IFTAppMintState, IFTAppState};

#[derive(Accounts)]
#[instruction(new_admin: Pubkey)]
pub struct SetAdmin<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    #[account(
        constraint = admin.key() == app_state.admin @ IFTError::UnauthorizedAdmin
    )]
    pub admin: Signer<'info>,
}

/// Transfer admin to `new_admin`.
/// TODO: consider a two-step transfer (propose + accept) to guard against typos.
pub fn set_admin(ctx: Context<SetAdmin>, new_admin: Pubkey) -> Result<()> {
    ctx.accounts.app_state.admin = new_admin;

    let clock = Clock::get()?;
    emit!(AdminUpdated {
        new_admin,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

/// Revoke mint authority from IFT and transfer it to a new authority.
#[derive(Accounts)]
pub struct RevokeMintAuthority<'info> {
    /// Global IFT app state (read-only, for admin check)
    #[account(
        seeds = [IFT_APP_STATE_SEED],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Per-mint IFT app state (for `mint_authority_bump`)
    #[account(
        seeds = [IFT_APP_MINT_STATE_SEED, mint.key().as_ref()],
        bump = app_mint_state.bump
    )]
    pub app_mint_state: Account<'info, IFTAppMintState>,

    /// SPL Token mint - authority will be transferred
    #[account(
        mut,
        address = app_mint_state.mint
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Current mint authority PDA (IFT's)
    /// CHECK: Derived PDA verified by seeds
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump = app_mint_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// New mint authority to receive ownership
    /// CHECK: Can be any pubkey chosen by admin
    pub new_mint_authority: AccountInfo<'info>,

    /// Admin signer
    #[account(
        constraint = admin.key() == app_state.admin @ IFTError::UnauthorizedAdmin
    )]
    pub admin: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

/// Revoke mint authority and transfer it to the specified new authority.
pub fn revoke_mint_authority(ctx: Context<RevokeMintAuthority>) -> Result<()> {
    let mint_key = ctx.accounts.mint.key();
    let mint_authority_bump = ctx.accounts.app_mint_state.mint_authority_bump;

    let seeds = &[
        MINT_AUTHORITY_SEED,
        mint_key.as_ref(),
        &[mint_authority_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    anchor_spl::token_interface::set_authority(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token_interface::SetAuthority {
                current_authority: ctx.accounts.mint_authority.to_account_info(),
                account_or_mint: ctx.accounts.mint.to_account_info(),
            },
            signer_seeds,
        ),
        anchor_spl::token_interface::spl_token_2022::instruction::AuthorityType::MintTokens,
        Some(ctx.accounts.new_mint_authority.key()),
    )?;

    let clock = Clock::get()?;
    emit!(MintAuthorityRevoked {
        mint: ctx.accounts.mint.key(),
        new_authority: ctx.accounts.new_mint_authority.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

/// Admin-callable mint instruction
#[derive(Accounts)]
#[instruction(msg: AdminMintMsg)]
pub struct AdminMint<'info> {
    /// Global IFT app state (read-only, for admin + pause check)
    #[account(
        seeds = [IFT_APP_STATE_SEED],
        bump = app_state.bump,
        constraint = !app_state.paused @ IFTError::TokenPaused,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Per-mint IFT app state (mut for rate limits)
    #[account(
        mut,
        seeds = [IFT_APP_MINT_STATE_SEED, mint.key().as_ref()],
        bump = app_mint_state.bump
    )]
    pub app_mint_state: Account<'info, IFTAppMintState>,

    /// SPL Token mint
    #[account(
        mut,
        address = app_mint_state.mint
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA that signs for minting
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump = app_mint_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// Receiver's token account (will be created if needed)
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = receiver_owner,
        associated_token::token_program = token_program,
    )]
    pub receiver_token_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Receiver who will own the minted tokens.
    #[account(
        constraint = receiver_owner.key() == msg.receiver @ IFTError::InvalidReceiver
    )]
    pub receiver_owner: AccountInfo<'info>,

    /// Admin signer
    #[account(
        constraint = admin.key() == app_state.admin @ IFTError::UnauthorizedAdmin
    )]
    pub admin: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Mint tokens to any account (admin only). Respects rate limits and pause state.
pub fn admin_mint(ctx: Context<AdminMint>, msg: AdminMintMsg) -> Result<()> {
    let clock = Clock::get()?;

    require!(msg.amount > 0, IFTError::ZeroAmount);

    check_and_update_mint_rate_limit(&mut ctx.accounts.app_mint_state, msg.amount, &clock)?;

    mint_to_account(
        &ctx.accounts.mint,
        &ctx.accounts.receiver_token_account,
        &ctx.accounts.mint_authority,
        ctx.accounts.app_mint_state.mint_authority_bump,
        &ctx.accounts.token_program,
        msg.amount,
    )?;

    emit!(AdminMintExecuted {
        mint: ctx.accounts.mint.key(),
        receiver: msg.receiver,
        amount: msg.amount,
        admin: ctx.accounts.admin.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::InstructionData;
    use rstest::rstest;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    use crate::test_utils::*;

    #[test]
    fn test_set_admin_success() {
        let mollusk = setup_mollusk();

        let admin = Pubkey::new_unique();
        let new_admin = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) = get_app_state_pda();

        let app_state_account =
            create_ift_app_state_account(app_state_bump, admin, Pubkey::new_unique());

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(admin, true),
            ],
            data: crate::instruction::SetAdmin { new_admin }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (admin, create_signer_account()),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "set_admin should succeed: {:?}",
            result.program_result
        );

        let updated_account = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == app_state_pda)
            .expect("app state should exist")
            .1
            .clone();
        let updated_state = deserialize_app_state(&updated_account);
        assert_eq!(updated_state.admin, new_admin, "Admin should be updated");
    }

    #[test]
    fn test_set_admin_unauthorized() {
        let mollusk = setup_mollusk();

        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let new_admin = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();

        let app_state_account =
            create_ift_app_state_account(app_state_bump, admin, Pubkey::new_unique());

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(unauthorized, true),
            ],
            data: crate::instruction::SetAdmin { new_admin }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (unauthorized, create_signer_account()),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[derive(Clone, Copy)]
    enum AdminMintErrorCase {
        Unauthorized,
        ZeroAmount,
        TokenPaused,
        RateLimitExceeded,
    }

    struct AdminMintTestConfig {
        amount: u64,
        use_real_admin: bool,
        paused: bool,
        daily_mint_limit: u64,
        rate_limit_daily_usage: u64,
    }

    impl From<AdminMintErrorCase> for AdminMintTestConfig {
        fn from(case: AdminMintErrorCase) -> Self {
            let default = Self {
                amount: 1000,
                use_real_admin: true,
                paused: false,
                daily_mint_limit: 0,
                rate_limit_daily_usage: 0,
            };

            match case {
                AdminMintErrorCase::Unauthorized => Self {
                    use_real_admin: false,
                    ..default
                },
                AdminMintErrorCase::ZeroAmount => Self {
                    amount: 0,
                    ..default
                },
                AdminMintErrorCase::TokenPaused => Self {
                    paused: true,
                    ..default
                },
                AdminMintErrorCase::RateLimitExceeded => Self {
                    daily_mint_limit: 100,
                    rate_limit_daily_usage: 100,
                    ..default
                },
            }
        }
    }

    fn run_admin_mint_error_test(case: AdminMintErrorCase) {
        let config = AdminMintTestConfig::from(case);
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let receiver = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();
        let (token_program_id, token_program_account) = create_token_program_account();

        let app_state_account = create_ift_app_state_account_with_options(
            app_state_bump,
            admin,
            Pubkey::new_unique(),
            config.paused,
        );

        let app_mint_state_account =
            create_ift_app_mint_state_account_full(IftAppMintStateParams {
                mint,
                bump: app_mint_state_bump,
                mint_authority_bump,
                daily_mint_limit: config.daily_mint_limit,
                rate_limit_day: 0,
                rate_limit_daily_usage: config.rate_limit_daily_usage,
            });

        let mint_account = create_mint_account(mint_authority_pda, 6);

        let receiver_token_pda = Pubkey::new_unique();
        let mut receiver_token_data = vec![0u8; 165];
        receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
        receiver_token_data[32..64].copy_from_slice(&receiver.to_bytes());
        let receiver_token_account = solana_sdk::account::Account {
            lamports: 1_000_000,
            data: receiver_token_data,
            owner: anchor_spl::token::ID,
            executable: false,
            rent_epoch: 0,
        };

        let associated_token_program_account = solana_sdk::account::Account {
            lamports: 1,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        };

        let signer_key = if config.use_real_admin {
            admin
        } else {
            unauthorized
        };

        let msg = crate::state::AdminMintMsg {
            receiver,
            amount: config.amount,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(app_mint_state_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(receiver_token_pda, false),
                AccountMeta::new_readonly(receiver, false),
                AccountMeta::new_readonly(signer_key, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(token_program_id, false),
                AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::AdminMint { msg }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (app_mint_state_pda, app_mint_state_account),
            (mint, mint_account),
            (mint_authority_pda, create_signer_account()),
            (receiver_token_pda, receiver_token_account),
            (receiver, create_signer_account()),
            (signer_key, create_signer_account()),
            (payer, create_signer_account()),
            (token_program_id, token_program_account),
            (
                anchor_spl::associated_token::ID,
                associated_token_program_account,
            ),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[rstest]
    #[case::unauthorized(AdminMintErrorCase::Unauthorized)]
    #[case::zero_amount(AdminMintErrorCase::ZeroAmount)]
    #[case::token_paused(AdminMintErrorCase::TokenPaused)]
    #[case::rate_limit_exceeded(AdminMintErrorCase::RateLimitExceeded)]
    fn test_admin_mint_validation(#[case] case: AdminMintErrorCase) {
        run_admin_mint_error_test(case);
    }

    #[test]
    fn test_revoke_mint_authority_unauthorized() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let new_mint_authority = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (token_program_id, token_program_account) = create_token_program_account();

        let app_state_account =
            create_ift_app_state_account(app_state_bump, admin, Pubkey::new_unique());

        let app_mint_state_account =
            create_ift_app_mint_state_account(mint, app_mint_state_bump, mint_authority_bump);

        let mint_account = create_mint_account(mint_authority_pda, 6);

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new_readonly(app_mint_state_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new_readonly(new_mint_authority, false),
                AccountMeta::new_readonly(unauthorized, true),
                AccountMeta::new_readonly(token_program_id, false),
            ],
            data: crate::instruction::RevokeMintAuthority {}.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (app_mint_state_pda, app_mint_state_account),
            (mint, mint_account),
            (mint_authority_pda, create_signer_account()),
            (new_mint_authority, create_signer_account()),
            (unauthorized, create_signer_account()),
            (token_program_id, token_program_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }
}
