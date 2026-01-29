use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTBridgeRemoved;
use crate::state::{IFTAppState, IFTBridge};

#[derive(Accounts)]
#[instruction(client_id: String)]
pub struct RemoveIFTBridge<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge to remove (close and refund rent)
    #[account(
        mut,
        close = payer,
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), client_id.as_bytes()],
        bump = ift_bridge.bump,
        constraint = ift_bridge.mint == app_state.mint @ IFTError::BridgeNotFound
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Authority with admin role
    pub authority: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn remove_ift_bridge(ctx: Context<RemoveIFTBridge>, client_id: String) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let clock = Clock::get()?;
    emit!(IFTBridgeRemoved {
        mint: ctx.accounts.app_state.mint,
        client_id,
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

    use crate::state::ChainOptions;
    use crate::test_utils::*;

    #[test]
    fn test_remove_ift_bridge_success() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let client_id = "07-tendermint-0";

        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (bridge_pda, bridge_bump) = get_bridge_pda(&mint, client_id);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let bridge_account = create_ift_bridge_account(
            mint,
            client_id,
            "0x1234",
            ChainOptions::Evm,
            bridge_bump,
            true,
        );

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(bridge_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::RemoveIftBridge {
                client_id: client_id.to_string(),
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (bridge_pda, bridge_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (instructions_sysvar, instructions_account),
            (payer, create_signer_account()),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "remove_ift_bridge should succeed: {:?}",
            result.program_result
        );

        let bridge_result = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == bridge_pda)
            .expect("bridge should exist")
            .1
            .clone();

        assert_eq!(
            bridge_result.lamports, 0,
            "Bridge lamports should be zero after close"
        );
    }

    #[test]
    fn test_remove_ift_bridge_unauthorized_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let client_id = "07-tendermint-0";

        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (bridge_pda, bridge_bump) = get_bridge_pda(&mint, client_id);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let bridge_account = create_ift_bridge_account(
            mint,
            client_id,
            "0x1234",
            ChainOptions::Evm,
            bridge_bump,
            true,
        );

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(bridge_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(unauthorized, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::RemoveIftBridge {
                client_id: client_id.to_string(),
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (bridge_pda, bridge_account),
            (access_manager_pda, access_manager_account),
            (unauthorized, create_signer_account()),
            (instructions_sysvar, instructions_account),
            (payer, create_signer_account()),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "remove_ift_bridge should fail for unauthorized user"
        );
    }

    #[test]
    fn test_remove_ift_bridge_mint_mismatch_fails() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let wrong_mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let client_id = "07-tendermint-0";

        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (bridge_pda, bridge_bump) = get_bridge_pda(&mint, client_id);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let bridge_account = create_ift_bridge_account(
            wrong_mint,
            client_id,
            "0x1234",
            ChainOptions::Evm,
            bridge_bump,
            true,
        );

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(bridge_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::RemoveIftBridge {
                client_id: client_id.to_string(),
            }
            .data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (bridge_pda, bridge_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (instructions_sysvar, instructions_account),
            (payer, create_signer_account()),
            (system_program, system_account),
        ];

        let checks = vec![mollusk_svm::result::Check::err(
            solana_sdk::program_error::ProgramError::Custom(13004),
        )];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
