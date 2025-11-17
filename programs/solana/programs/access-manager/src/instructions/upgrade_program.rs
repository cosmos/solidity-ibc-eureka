use crate::errors::AccessManagerError;
use crate::events::ProgramUpgradedEvent;
use crate::state::AccessManager;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;
use solana_ibc_types::roles;

#[derive(Accounts)]
#[instruction(target_program: Pubkey)]
pub struct UpgradeProgram<'info> {
    #[account(
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    /// CHECK: Validated as executable program account
    /// Must be writable because BPF Loader Upgradeable requires both program and programdata
    /// accounts to be writable during upgrade. The program account contains metadata and a
    /// pointer to the programdata account, which may be updated during the upgrade process.
    #[account(
        mut,
        executable,
        constraint = program.key() == target_program @ AccessManagerError::InvalidUpgradeAuthority
    )]
    pub program: AccountInfo<'info>,

    /// CHECK: Validated via BPF Loader constraints
    #[account(mut)]
    pub program_data: AccountInfo<'info>,

    /// CHECK: Validated via BPF Loader as buffer account
    #[account(mut)]
    pub buffer: AccountInfo<'info>,

    /// CHECK: Validated via seeds constraint
    #[account(
        mut,
        seeds = [AccessManager::UPGRADE_AUTHORITY_SEED, target_program.as_ref()],
        bump
    )]
    pub upgrade_authority: AccountInfo<'info>,

    /// CHECK: Can be any account to receive refunded rent
    #[account(mut)]
    pub spill: AccountInfo<'info>,

    pub authority: Signer<'info>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    /// CHECK: Must be BPF Loader Upgradeable program ID
    #[account(address = bpf_loader_upgradeable::ID)]
    pub bpf_loader_upgradeable: AccountInfo<'info>,

    pub rent: Sysvar<'info, Rent>,

    pub clock: Sysvar<'info, Clock>,
}

pub fn upgrade_program(ctx: Context<UpgradeProgram>, target_program: Pubkey) -> Result<()> {
    crate::helpers::require_role(
        &ctx.accounts.access_manager.to_account_info(),
        roles::UPGRADER_ROLE,
        &ctx.accounts.authority.to_account_info(),
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let (upgrade_authority_pda, bump) =
        AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

    let upgrade_ix = bpf_loader_upgradeable::upgrade(
        &ctx.accounts.program.key(),
        &ctx.accounts.buffer.key(),
        &upgrade_authority_pda,
        &ctx.accounts.spill.key(),
    );

    anchor_lang::solana_program::program::invoke_signed(
        &upgrade_ix,
        &[
            ctx.accounts.program_data.to_account_info(),
            ctx.accounts.program.to_account_info(),
            ctx.accounts.buffer.to_account_info(),
            ctx.accounts.spill.to_account_info(),
            ctx.accounts.rent.to_account_info(),
            ctx.accounts.clock.to_account_info(),
            ctx.accounts.upgrade_authority.to_account_info(),
        ],
        &[&[
            AccessManager::UPGRADE_AUTHORITY_SEED,
            target_program.as_ref(),
            &[bump],
        ]],
    )?;

    let clock = Clock::get()?;
    emit!(ProgramUpgradedEvent {
        program: target_program,
        authority: ctx.accounts.authority.key(),
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Program {} upgraded by {}",
        target_program,
        ctx.accounts.authority.key()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use mollusk_svm::result::Check;
    use solana_sdk::{account::Account, instruction::AccountMeta};

    fn setup_upgrade_test(
        admin: Pubkey,
        upgrader: Pubkey,
        target_program: Pubkey,
    ) -> (
        Pubkey,
        Account,
        Pubkey,
        Pubkey,
        Pubkey,
        Pubkey,
        Vec<AccountMeta>,
    ) {
        let (access_manager_pda, mut access_manager_account) =
            create_initialized_access_manager(admin);

        let mut access_manager_data = get_account_data::<AccessManager>(&access_manager_account);
        access_manager_data
            .grant_role(roles::UPGRADER_ROLE, upgrader)
            .unwrap();
        access_manager_account.data = serialize_account(&access_manager_data);

        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

        let program_data_address = Pubkey::new_unique();
        let buffer = Pubkey::new_unique();
        let spill = Pubkey::new_unique();

        let account_metas = build_upgrade_account_metas(
            access_manager_pda,
            target_program,
            program_data_address,
            buffer,
            upgrade_authority_pda,
            spill,
            upgrader,
        );

        (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            buffer,
            spill,
            account_metas,
        )
    }

    fn create_program_accounts(
        target_program: Pubkey,
        program_data_address: Pubkey,
        buffer: Pubkey,
        upgrade_authority_pda: Pubkey,
        spill: Pubkey,
        upgrader: Pubkey,
    ) -> Vec<(Pubkey, Account)> {
        vec![
            (
                target_program,
                Account {
                    lamports: 1_000_000,
                    data: vec![],
                    owner: bpf_loader_upgradeable::ID,
                    executable: true,
                    ..Default::default()
                },
            ),
            (
                program_data_address,
                Account {
                    lamports: 1_000_000,
                    data: vec![0; 100],
                    owner: bpf_loader_upgradeable::ID,
                    ..Default::default()
                },
            ),
            (
                buffer,
                Account {
                    lamports: 1_000_000,
                    data: vec![0; 100],
                    owner: bpf_loader_upgradeable::ID,
                    ..Default::default()
                },
            ),
            (
                upgrade_authority_pda,
                Account {
                    lamports: 1_000_000,
                    owner: crate::ID,
                    ..Default::default()
                },
            ),
            (
                spill,
                Account {
                    lamports: 1_000_000,
                    owner: solana_sdk::system_program::ID,
                    ..Default::default()
                },
            ),
            (upgrader, create_signer_account()),
            (
                bpf_loader_upgradeable::ID,
                Account {
                    lamports: 1_000_000,
                    executable: true,
                    owner: solana_sdk::native_loader::ID,
                    ..Default::default()
                },
            ),
            create_rent_sysvar_account(),
            create_clock_sysvar_account(),
        ]
    }

    fn build_upgrade_account_metas(
        access_manager_pda: Pubkey,
        target_program: Pubkey,
        program_data_address: Pubkey,
        buffer: Pubkey,
        upgrade_authority_pda: Pubkey,
        spill: Pubkey,
        authority: Pubkey,
    ) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new(target_program, false),
            AccountMeta::new(program_data_address, false),
            AccountMeta::new(buffer, false),
            AccountMeta::new(upgrade_authority_pda, false),
            AccountMeta::new(spill, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(bpf_loader_upgradeable::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::clock::ID, false),
        ]
    }

    #[allow(clippy::too_many_arguments)]
    fn build_upgrade_instruction_and_accounts(
        access_manager_pda: Pubkey,
        access_manager_account: Account,
        target_program: Pubkey,
        program_data_address: Pubkey,
        buffer: Pubkey,
        upgrade_authority_pda: Pubkey,
        spill: Pubkey,
        authority: Pubkey,
        sysvar_account: (Pubkey, Account),
    ) -> (solana_sdk::instruction::Instruction, Vec<(Pubkey, Account)>) {
        let account_metas = build_upgrade_account_metas(
            access_manager_pda,
            target_program,
            program_data_address,
            buffer,
            upgrade_authority_pda,
            spill,
            authority,
        );

        let instruction = build_instruction(
            crate::instruction::UpgradeProgram { target_program },
            account_metas,
        );

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_program_accounts(
            target_program,
            program_data_address,
            buffer,
            upgrade_authority_pda,
            spill,
            authority,
        ));
        accounts.push(sysvar_account);

        (instruction, accounts)
    }

    // Note: This test cannot fully succeed in Mollusk because invoke_signed to bpf_loader_upgradeable
    // references additional accounts not available in the unit test environment.
    // The instruction logic is validated by the authorization tests.
    // Full upgrade testing should be done in integration tests.
    #[test]
    #[ignore = "Requires full integration test setup with BPF Loader"]
    fn test_upgrade_program_success() {
        let admin = Pubkey::new_unique();
        let upgrader = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();

        let (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            buffer,
            spill,
            account_metas,
        ) = setup_upgrade_test(admin, upgrader, target_program);

        let instruction = build_instruction(
            crate::instruction::UpgradeProgram { target_program },
            account_metas,
        );

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_program_accounts(
            target_program,
            program_data_address,
            buffer,
            upgrade_authority_pda,
            spill,
            upgrader,
        ));
        accounts.push(create_instructions_sysvar_account_with_caller(crate::ID));

        let mollusk = setup_mollusk();
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_upgrade_program_not_upgrader() {
        let admin = Pubkey::new_unique();
        let non_upgrader = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

        let program_data_address = Pubkey::new_unique();
        let buffer = Pubkey::new_unique();
        let spill = Pubkey::new_unique();

        let (instruction, accounts) = build_upgrade_instruction_and_accounts(
            access_manager_pda,
            access_manager_account,
            target_program,
            program_data_address,
            buffer,
            upgrade_authority_pda,
            spill,
            non_upgrader,
            create_instructions_sysvar_account_with_caller(crate::ID),
        );

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_upgrade_program_cpi_rejection() {
        let admin = Pubkey::new_unique();
        let upgrader = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();

        let (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            buffer,
            spill,
            account_metas,
        ) = setup_upgrade_test(admin, upgrader, target_program);

        let instruction = build_instruction(
            crate::instruction::UpgradeProgram { target_program },
            account_metas,
        );

        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_program_accounts(
            target_program,
            program_data_address,
            buffer,
            upgrade_authority_pda,
            spill,
            upgrader,
        ));
        accounts.push(cpi_sysvar_account);

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_access_manager_cpi_rejection_error()],
        );
    }

    #[test]
    fn test_upgrade_program_fake_sysvar_wormhole_attack() {
        let admin = Pubkey::new_unique();
        let upgrader = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();

        let (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            buffer,
            spill,
            account_metas,
        ) = setup_upgrade_test(admin, upgrader, target_program);

        let instruction = build_instruction(
            crate::instruction::UpgradeProgram { target_program },
            account_metas,
        );

        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_program_accounts(
            target_program,
            program_data_address,
            buffer,
            upgrade_authority_pda,
            spill,
            upgrader,
        ));
        accounts.push(fake_sysvar_account);

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_upgrade_program_wrong_pda() {
        let admin = Pubkey::new_unique();
        let upgrader = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();

        let (access_manager_pda, mut access_manager_account) =
            create_initialized_access_manager(admin);

        let mut access_manager_data = get_account_data::<AccessManager>(&access_manager_account);
        access_manager_data
            .grant_role(roles::UPGRADER_ROLE, upgrader)
            .unwrap();
        access_manager_account.data = serialize_account(&access_manager_data);

        let wrong_upgrade_authority = Pubkey::new_unique();
        let program_data_address = Pubkey::new_unique();
        let buffer = Pubkey::new_unique();
        let spill = Pubkey::new_unique();

        let (instruction, accounts) = build_upgrade_instruction_and_accounts(
            access_manager_pda,
            access_manager_account,
            target_program,
            program_data_address,
            buffer,
            wrong_upgrade_authority,
            spill,
            upgrader,
            create_instructions_sysvar_account_with_caller(crate::ID),
        );

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            anchor_lang::error::ErrorCode::ConstraintSeeds as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
