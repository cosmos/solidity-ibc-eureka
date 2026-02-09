use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::constants::*;
use crate::events::SplTokenCreated;
use crate::state::{AccountVersion, IFTAppState};

// TODO: Add create and init spl token
#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateSplToken<'info> {
    /// IFT app state PDA (to be created)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTAppState::INIT_SPACE,
        seeds = [IFT_APP_STATE_SEED, mint.key().as_ref()],
        bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// SPL Token mint (created by IFT with PDA as authority)
    #[account(
        init,
        payer = payer,
        mint::decimals = decimals,
        mint::authority = mint_authority,
    )]
    pub mint: Account<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA set as mint authority
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump
    )]
    pub mint_authority: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// TODO: check compatibility with token 2022 and write a test for it
pub fn create_spl_token(
    ctx: Context<CreateSplToken>,
    decimals: u8,
    admin: Pubkey,
    gmp_program: Pubkey,
) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;
    app_state.version = AccountVersion::V1;
    app_state.bump = ctx.bumps.app_state;
    app_state.mint = ctx.accounts.mint.key();
    app_state.mint_authority_bump = ctx.bumps.mint_authority;
    app_state.admin = admin;
    app_state.gmp_program = gmp_program;

    let clock = Clock::get()?;
    emit!(SplTokenCreated {
        mint: ctx.accounts.mint.key(),
        decimals,
        admin,
        gmp_program,
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

    use crate::state::IFTAppState;
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

        // Use wrong mint for PDA derivation
        let (wrong_app_state_pda, _) = get_app_state_pda(&wrong_mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(8 + IFTAppState::INIT_SPACE),
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
                AccountMeta::new(wrong_app_state_pda, false),
                AccountMeta::new(mint, true), // mint must sign for init
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::CreateSplToken {
                decimals: 6,
                admin,
                gmp_program,
            }
            .data(),
        };

        let accounts = vec![
            (wrong_app_state_pda, app_state_account),
            (mint, create_empty_mint_account()),
            (mint_authority_pda, mint_authority_account),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "create_spl_token should fail with wrong PDA seeds"
        );
    }
}
