use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::constants::*;
use crate::events::MintAuthorityRevoked;
use crate::state::IFTAppState;

#[derive(Accounts)]
pub struct SetAccessManager<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Admin with admin role
    pub admin: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn set_access_manager(
    ctx: Context<SetAccessManager>,
    new_access_manager: Pubkey,
) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    ctx.accounts.app_state.access_manager = new_access_manager;

    Ok(())
}

/// Revoke mint authority from IFT and transfer it to a new authority.
#[derive(Accounts)]
pub struct RevokeMintAuthority<'info> {
    /// IFT app state (will be closed)
    #[account(
        mut,
        close = payer,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// SPL Token mint - authority will be transferred
    #[account(
        mut,
        address = app_state.mint
    )]
    pub mint: Account<'info, Mint>,

    /// Current mint authority PDA (IFT's)
    /// CHECK: Derived PDA verified by seeds
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump = app_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// New mint authority to receive ownership
    /// CHECK: Can be any pubkey chosen by admin
    pub new_mint_authority: AccountInfo<'info>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Admin signer (must have ADMIN_ROLE)
    pub admin: Signer<'info>,

    /// Payer receives rent from closed app_state
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Instructions sysvar for access manager verification
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

/// Revoke mint authority and close IFT app state.
/// Transfers mint authority back to the specified new authority.
pub fn revoke_mint_authority(ctx: Context<RevokeMintAuthority>) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let mint_key = ctx.accounts.mint.key();
    let mint_authority_bump = ctx.accounts.app_state.mint_authority_bump;

    let seeds = &[
        MINT_AUTHORITY_SEED,
        mint_key.as_ref(),
        &[mint_authority_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    anchor_spl::token::set_authority(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::SetAuthority {
                current_authority: ctx.accounts.mint_authority.to_account_info(),
                account_or_mint: ctx.accounts.mint.to_account_info(),
            },
            signer_seeds,
        ),
        anchor_spl::token::spl_token::instruction::AuthorityType::MintTokens,
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

#[cfg(test)]
mod tests {
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    use crate::test_utils::*;

    #[test]
    fn test_set_access_manager_success() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
            ],
            data: crate::instruction::SetAccessManager { new_access_manager }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (instructions_sysvar, instructions_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "set_access_manager should succeed: {:?}",
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
        assert_eq!(
            updated_state.access_manager, new_access_manager,
            "Access manager should be updated"
        );
    }

    #[test]
    fn test_set_access_manager_unauthorized_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(unauthorized, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
            ],
            data: crate::instruction::SetAccessManager { new_access_manager }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (unauthorized, create_signer_account()),
            (instructions_sysvar, instructions_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "set_access_manager should fail for unauthorized user"
        );
    }

    #[test]
    fn test_revoke_mint_authority_unauthorized_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let new_mint_authority = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (mint_authority_pda, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
        let (token_program_id, token_program_account) = create_token_program_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let mint_account = create_mint_account(mint_authority_pda, 6);

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new_readonly(new_mint_authority, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(unauthorized, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
                AccountMeta::new_readonly(token_program_id, false),
            ],
            data: crate::instruction::RevokeMintAuthority {}.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (mint, mint_account),
            (mint_authority_pda, create_signer_account()),
            (new_mint_authority, create_signer_account()),
            (access_manager_pda, access_manager_account),
            (unauthorized, create_signer_account()),
            (payer, create_signer_account()),
            (instructions_sysvar, instructions_account),
            (token_program_id, token_program_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "revoke_mint_authority should fail for unauthorized user"
        );
    }
}
