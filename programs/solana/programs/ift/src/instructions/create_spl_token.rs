use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::*;
use crate::events::SplTokenCreated;
use crate::state::{AccountVersion, IFTAppMintState, IFTAppState};

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateSplToken<'info> {
    /// Global IFT app state (must exist)
    #[account(
        seeds = [IFT_APP_STATE_SEED],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Per-mint IFT app state PDA (to be created)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTAppMintState::INIT_SPACE,
        seeds = [IFT_APP_MINT_STATE_SEED, mint.key().as_ref()],
        bump
    )]
    pub app_mint_state: Account<'info, IFTAppMintState>,

    /// SPL Token mint (created by IFT with PDA as authority)
    #[account(
        init,
        payer = payer,
        mint::decimals = decimals,
        mint::authority = mint_authority,
        mint::token_program = token_program,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA set as mint authority
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump
    )]
    pub mint_authority: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn create_spl_token(ctx: Context<CreateSplToken>, decimals: u8) -> Result<()> {
    let app_mint_state = &mut ctx.accounts.app_mint_state;
    app_mint_state.version = AccountVersion::V1;
    app_mint_state.bump = ctx.bumps.app_mint_state;
    app_mint_state.mint = ctx.accounts.mint.key();
    app_mint_state.mint_authority_bump = ctx.bumps.mint_authority;

    let clock = Clock::get()?;
    emit!(SplTokenCreated {
        mint: ctx.accounts.mint.key(),
        decimals,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{InstructionData, Space};
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        rent::Rent,
    };

    use solana_sdk::program_pack::Pack;

    use crate::state::IFTAppMintState;
    use crate::test_utils::*;

    fn create_empty_mint_account() -> solana_sdk::account::Account {
        solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(82),
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    #[test]
    fn test_create_spl_token_wrong_pda_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let wrong_mint = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        // Use wrong mint for per-mint PDA derivation
        let (wrong_app_mint_state_pda, _) = get_app_mint_state_pda(&wrong_mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();

        let app_mint_state_account = solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(8 + IFTAppMintState::INIT_SPACE),
            data: vec![],
            owner: solana_sdk::system_program::ID,
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
                AccountMeta::new(wrong_app_mint_state_pda, false),
                AccountMeta::new(mint, true), // mint must sign for init
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::CreateSplToken { decimals: 6 }.data(),
        };

        let accounts = vec![
            (
                app_state_pda,
                create_ift_app_state_account(app_state_bump, admin, gmp_program),
            ),
            (wrong_app_mint_state_pda, app_mint_state_account),
            (mint, create_empty_mint_account()),
            (mint_authority_pda, mint_authority_account),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert_eq!(
            result.program_result,
            Err(solana_sdk::instruction::InstructionError::Custom(
                anchor_lang::error::ErrorCode::ConstraintSeeds as u32,
            ))
            .into(),
        );
    }

    #[test]
    fn test_create_spl_token_success() {
        let mollusk = setup_mollusk_with_token();

        let mint = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, _) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();
        let (token_program_id, token_program_account) = token_program_keyed_account();

        let app_mint_state_account = solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(8 + IFTAppMintState::INIT_SPACE),
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(app_mint_state_pda, false),
                AccountMeta::new(mint, true),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(token_program_id, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::CreateSplToken { decimals: 6 }.data(),
        };

        let accounts = vec![
            (
                app_state_pda,
                create_ift_app_state_account(app_state_bump, admin, gmp_program),
            ),
            (app_mint_state_pda, app_mint_state_account),
            (mint, create_empty_mint_account()),
            (mint_authority_pda, create_uninitialized_pda()),
            (payer, create_signer_account()),
            (token_program_id, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "create_spl_token should succeed: {:?}",
            result.program_result
        );

        let (_, mint_acc) = &result.resulting_accounts[2];
        let created_mint =
            anchor_spl::token::spl_token::state::Mint::unpack(&mint_acc.data).expect("valid mint");
        assert_eq!(created_mint.decimals, 6);
        assert_eq!(
            created_mint.mint_authority,
            solana_sdk::program_option::COption::Some(mint_authority_pda),
        );

        let (_, mint_state_acc) = &result.resulting_accounts[1];
        let mint_state = deserialize_app_mint_state(mint_state_acc);
        assert_eq!(mint_state.mint, mint);
    }

    #[test]
    fn test_create_spl_token_zero_decimals_success() {
        let mollusk = setup_mollusk_with_token();

        let mint = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, _) = get_app_mint_state_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();
        let (token_program_id, token_program_account) = token_program_keyed_account();

        let app_mint_state_account = solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(8 + IFTAppMintState::INIT_SPACE),
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(app_mint_state_pda, false),
                AccountMeta::new(mint, true),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(token_program_id, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::CreateSplToken { decimals: 0 }.data(),
        };

        let accounts = vec![
            (
                app_state_pda,
                create_ift_app_state_account(app_state_bump, admin, gmp_program),
            ),
            (app_mint_state_pda, app_mint_state_account),
            (mint, create_empty_mint_account()),
            (mint_authority_pda, create_uninitialized_pda()),
            (payer, create_signer_account()),
            (token_program_id, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "create_spl_token with 0 decimals should succeed: {:?}",
            result.program_result
        );

        let (_, mint_acc) = &result.resulting_accounts[2];
        let created_mint =
            anchor_spl::token::spl_token::state::Mint::unpack(&mint_acc.data).expect("valid mint");
        assert_eq!(created_mint.decimals, 0);
    }
}
