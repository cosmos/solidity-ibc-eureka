use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTAppInitialized;
use crate::state::{AccountVersion, IFTAppState};

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct Initialize<'info> {
    /// IFT app state PDA (to be created)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTAppState::INIT_SPACE,
        seeds = [IFT_APP_STATE_SEED, mint.key().as_ref()],
        bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// SPL Token mint (must already exist, IFT will take mint authority)
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    /// Mint authority PDA - will become the mint authority
    /// CHECK: Derived PDA that will be set as mint authority
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump
    )]
    pub mint_authority: AccountInfo<'info>,

    // TODO: IFT creates the mint during init, there's no need for current_mint_authority - the mint would be created with IFT's PDA as authority from the start
    /// Current mint authority (must sign to transfer authority)
    pub current_mint_authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn initialize(
    ctx: Context<Initialize>,
    decimals: u8,
    access_manager: Pubkey,
    gmp_program: Pubkey,
) -> Result<()> {
    let mint = &ctx.accounts.mint;

    require!(mint.decimals == decimals, IFTError::DecimalsMismatch);
    require!(
        mint.mint_authority
            .contains(&ctx.accounts.current_mint_authority.key()),
        IFTError::InvalidMintAuthority
    );

    // Transfer mint authority to IFT PDA
    let cpi_accounts = anchor_spl::token::SetAuthority {
        account_or_mint: ctx.accounts.mint.to_account_info(),
        current_authority: ctx.accounts.current_mint_authority.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

    anchor_spl::token::set_authority(
        cpi_ctx,
        anchor_spl::token::spl_token::instruction::AuthorityType::MintTokens,
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
    emit!(IFTAppInitialized {
        mint: ctx.accounts.mint.key(),
        decimals,
        access_manager,
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

    fn create_mock_mint_account(
        decimals: u8,
        mint_authority: Pubkey,
    ) -> solana_sdk::account::Account {
        let mut data = vec![0u8; 82];

        // mint_authority = Some(mint_authority)
        data[0..4].copy_from_slice(&1u32.to_le_bytes());
        data[4..36].copy_from_slice(&mint_authority.to_bytes());

        // supply = 0
        data[36..44].copy_from_slice(&0u64.to_le_bytes());

        // decimals
        data[44] = decimals;

        // is_initialized = true
        data[45] = 1;

        // freeze_authority = None
        data[46..50].copy_from_slice(&0u32.to_le_bytes());

        solana_sdk::account::Account {
            lamports: 1_000_000,
            data,
            owner: anchor_spl::token::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    #[test]
    fn test_initialize_decimals_mismatch_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let current_mint_authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let access_manager = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, _) = get_app_state_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();

        let mint_account = create_mock_mint_account(6, current_mint_authority);

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
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new_readonly(current_mint_authority, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::Initialize {
                decimals: 9, // Wrong! Mint has 6
                access_manager,
                gmp_program,
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (mint, mint_account),
            (mint_authority_pda, mint_authority_account),
            (current_mint_authority, create_signer_account()),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "initialize should fail with decimals mismatch"
        );
    }

    #[test]
    fn test_initialize_no_authority_signer_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let current_mint_authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let access_manager = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, _) = get_app_state_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();

        let mint_account = create_mock_mint_account(6, current_mint_authority);

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
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new_readonly(current_mint_authority, false), // NOT a signer!
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::Initialize {
                decimals: 6,
                access_manager,
                gmp_program,
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (mint, mint_account),
            (mint_authority_pda, mint_authority_account),
            (current_mint_authority, create_signer_account()),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "initialize should fail when current_mint_authority is not a signer"
        );
    }

    #[test]
    fn test_initialize_wrong_pda_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let wrong_mint = Pubkey::new_unique();
        let current_mint_authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let access_manager = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (wrong_app_state_pda, _) = get_app_state_pda(&wrong_mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();

        let mint_account = create_mock_mint_account(6, current_mint_authority);

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
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new_readonly(current_mint_authority, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::Initialize {
                decimals: 6,
                access_manager,
                gmp_program,
            }
            .data(),
        };

        let accounts = vec![
            (wrong_app_state_pda, app_state_account),
            (mint, mint_account),
            (mint_authority_pda, mint_authority_account),
            (current_mint_authority, create_signer_account()),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "initialize should fail with wrong PDA seeds"
        );
    }

    #[test]
    fn test_initialize_wrong_mint_owner_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let current_mint_authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let access_manager = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, _) = get_app_state_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();

        let mut mint_account = create_mock_mint_account(6, current_mint_authority);
        mint_account.owner = Pubkey::new_unique(); // Wrong owner!

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
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new_readonly(current_mint_authority, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::Initialize {
                decimals: 6,
                access_manager,
                gmp_program,
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (mint, mint_account),
            (mint_authority_pda, mint_authority_account),
            (current_mint_authority, create_signer_account()),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "initialize should fail when mint is not owned by token program"
        );
    }

    #[test]
    fn test_initialize_wrong_mint_authority_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let actual_mint_authority = Pubkey::new_unique();
        let wrong_mint_authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let access_manager = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, _) = get_app_state_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (system_program, system_account) = create_system_program_account();

        let mint_account = create_mock_mint_account(6, actual_mint_authority);

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
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new_readonly(wrong_mint_authority, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::Initialize {
                decimals: 6,
                access_manager,
                gmp_program,
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (mint, mint_account),
            (mint_authority_pda, mint_authority_account),
            (wrong_mint_authority, create_signer_account()),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "initialize should fail when signer is not the actual mint authority"
        );
    }
}
