use crate::errors::AccessManagerError;
use crate::events::{
    UpgradeAuthorityTransferCancelledEvent, UpgradeAuthorityTransferProposedEvent,
    UpgradeAuthorityTransferredEvent,
};
use crate::helpers::require_admin;
use crate::state::AccessManager;
use crate::types::PendingAuthorityTransfer;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;

// ── Propose ──────────────────────────────────────────────────────────────────

/// Proposes transferring a target program's BPF Loader upgrade authority from
/// this access manager's PDA to a new authority. Requires admin authorization.
///
/// The proposed transfer must be accepted by the new authority via
/// `accept_upgrade_authority_transfer` before it takes effect.
#[derive(Accounts)]
pub struct ProposeUpgradeAuthorityTransfer<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    pub admin: Signer<'info>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn propose_upgrade_authority_transfer(
    ctx: Context<ProposeUpgradeAuthorityTransfer>,
    target_program: Pubkey,
    new_authority: Pubkey,
) -> Result<()> {
    require_admin(
        &ctx.accounts.access_manager.to_account_info(),
        &ctx.accounts.admin.to_account_info(),
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    require!(
        new_authority != Pubkey::default(),
        AccessManagerError::ZeroAccount
    );

    let (upgrade_authority_pda, _) =
        AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
    require!(
        new_authority != upgrade_authority_pda,
        AccessManagerError::SelfTransfer
    );

    require!(
        ctx.accounts
            .access_manager
            .pending_authority_transfer
            .is_none(),
        AccessManagerError::PendingTransferAlreadyExists
    );

    ctx.accounts.access_manager.pending_authority_transfer = Some(PendingAuthorityTransfer {
        target_program,
        new_authority,
    });

    emit!(UpgradeAuthorityTransferProposedEvent {
        program: target_program,
        current_authority: upgrade_authority_pda,
        proposed_authority: new_authority,
        proposed_by: ctx.accounts.admin.key(),
    });

    Ok(())
}

// ── Accept ───────────────────────────────────────────────────────────────────

/// Accepts a pending upgrade authority transfer by executing the BPF Loader
/// `SetAuthority` CPI. Must be signed by the proposed new authority.
///
/// No CPI restriction: supports both keypair signers and multisig/PDA callers.
#[derive(Accounts)]
#[instruction(target_program: Pubkey)]
pub struct AcceptUpgradeAuthorityTransfer<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    /// The target program's data account (BPF Loader Upgradeable PDA).
    /// CHECK: Validated via BPF Loader seeds derivation from `target_program`
    #[account(
        mut,
        seeds = [target_program.as_ref()],
        bump,
        seeds::program = bpf_loader_upgradeable::ID
    )]
    pub program_data: AccountInfo<'info>,

    /// `AccessManager`'s PDA that acts as the current upgrade authority.
    /// CHECK: Validated via seeds constraint
    #[account(
        seeds = [AccessManager::UPGRADE_AUTHORITY_SEED, target_program.as_ref()],
        bump
    )]
    pub upgrade_authority: AccountInfo<'info>,

    pub new_authority: Signer<'info>,

    /// CHECK: Must be BPF Loader Upgradeable program ID
    #[account(address = bpf_loader_upgradeable::ID)]
    pub bpf_loader_upgradeable: AccountInfo<'info>,
}

pub fn accept_upgrade_authority_transfer(
    ctx: Context<AcceptUpgradeAuthorityTransfer>,
    target_program: Pubkey,
) -> Result<()> {
    let pending = ctx
        .accounts
        .access_manager
        .pending_authority_transfer
        .as_ref()
        .ok_or_else(|| error!(AccessManagerError::NoPendingTransfer))?;

    require!(
        pending.target_program == target_program,
        AccessManagerError::PendingTransferMismatch
    );
    require!(
        pending.new_authority == ctx.accounts.new_authority.key(),
        AccessManagerError::AuthorityMismatch
    );

    let (upgrade_authority_pda, bump) =
        AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

    let set_authority_ix = bpf_loader_upgradeable::set_upgrade_authority(
        &target_program,
        &upgrade_authority_pda,
        Some(&ctx.accounts.new_authority.key()),
    );

    // BPF Loader SetAuthority expects:
    //   [0] programdata_address  (writable, non-signer)
    //   [1] current_authority    (read-only, signer via PDA)
    //   [2] new_authority        (read-only, non-signer)
    anchor_lang::solana_program::program::invoke_signed(
        &set_authority_ix,
        &[
            ctx.accounts.program_data.to_account_info(),
            ctx.accounts.upgrade_authority.to_account_info(),
            ctx.accounts.new_authority.to_account_info(),
        ],
        &[&[
            AccessManager::UPGRADE_AUTHORITY_SEED,
            target_program.as_ref(),
            &[bump],
        ]],
    )?;

    let new_authority = ctx.accounts.new_authority.key();
    ctx.accounts.access_manager.pending_authority_transfer = None;

    emit!(UpgradeAuthorityTransferredEvent {
        program: target_program,
        old_authority: upgrade_authority_pda,
        new_authority,
        accepted_by: new_authority,
    });

    Ok(())
}

// ── Cancel ───────────────────────────────────────────────────────────────────

/// Cancels a pending upgrade authority transfer. Requires admin authorization.
#[derive(Accounts)]
pub struct CancelUpgradeAuthorityTransfer<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    pub admin: Signer<'info>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn cancel_upgrade_authority_transfer(
    ctx: Context<CancelUpgradeAuthorityTransfer>,
    target_program: Pubkey,
) -> Result<()> {
    require_admin(
        &ctx.accounts.access_manager.to_account_info(),
        &ctx.accounts.admin.to_account_info(),
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let pending = ctx
        .accounts
        .access_manager
        .pending_authority_transfer
        .as_ref()
        .ok_or_else(|| error!(AccessManagerError::NoPendingTransfer))?;

    require!(
        pending.target_program == target_program,
        AccessManagerError::PendingTransferMismatch
    );

    let cancelled_authority = pending.new_authority;

    ctx.accounts.access_manager.pending_authority_transfer = None;

    emit!(UpgradeAuthorityTransferCancelledEvent {
        program: target_program,
        cancelled_authority,
        cancelled_by: ctx.accounts.admin.key(),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use mollusk_svm::result::Check;
    use solana_ibc_types::roles;
    use solana_sdk::{account::Account, instruction::AccountMeta};

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn create_access_manager_with_pending(
        admin: Pubkey,
        target_program: Pubkey,
        new_authority: Pubkey,
    ) -> (Pubkey, Account) {
        let (pda, _) = Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);
        let am = AccessManager {
            roles: vec![crate::types::RoleData {
                role_id: roles::ADMIN_ROLE,
                members: vec![admin],
            }],
            whitelisted_programs: vec![],
            pending_authority_transfer: Some(PendingAuthorityTransfer {
                target_program,
                new_authority,
            }),
        };
        let mut data = vec![0u8; 8 + AccessManager::INIT_SPACE];
        data[0..8].copy_from_slice(AccessManager::DISCRIMINATOR);
        am.serialize(&mut &mut data[8..]).unwrap();
        (
            pda,
            Account {
                lamports: 1_000_000,
                data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
    }

    fn build_propose_account_metas(
        access_manager_pda: Pubkey,
        authority: Pubkey,
    ) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(access_manager_pda, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ]
    }

    fn build_cancel_account_metas(
        access_manager_pda: Pubkey,
        authority: Pubkey,
    ) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(access_manager_pda, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ]
    }

    fn derive_program_data(target_program: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID).0
    }

    fn build_accept_account_metas(
        access_manager_pda: Pubkey,
        program_data_address: Pubkey,
        upgrade_authority_pda: Pubkey,
        new_authority: Pubkey,
    ) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(access_manager_pda, false),
            AccountMeta::new(program_data_address, false),
            AccountMeta::new_readonly(upgrade_authority_pda, false),
            AccountMeta::new_readonly(new_authority, true),
            AccountMeta::new_readonly(bpf_loader_upgradeable::ID, false),
        ]
    }

    fn create_accept_accounts(
        program_data_address: Pubkey,
        upgrade_authority_pda: Pubkey,
        new_authority: Pubkey,
    ) -> Vec<(Pubkey, Account)> {
        vec![
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
                upgrade_authority_pda,
                Account {
                    lamports: 1_000_000,
                    owner: crate::ID,
                    ..Default::default()
                },
            ),
            (new_authority, create_signer_account()),
            (
                bpf_loader_upgradeable::ID,
                Account {
                    lamports: 1_000_000,
                    executable: true,
                    owner: solana_sdk::native_loader::ID,
                    ..Default::default()
                },
            ),
        ]
    }

    // ── Propose tests ────────────────────────────────────────────────────────

    #[test]
    fn test_propose_success() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) = create_initialized_access_manager(admin);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority,
            },
            build_propose_account_metas(pda, admin),
        );

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        let result =
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

        let am = get_access_manager_from_result(&result, &pda);
        assert_eq!(
            am.pending_authority_transfer,
            Some(PendingAuthorityTransfer {
                target_program,
                new_authority,
            })
        );
    }

    #[test]
    fn test_propose_not_admin() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) = create_initialized_access_manager(admin);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority,
            },
            build_propose_account_metas(pda, non_admin),
        );

        let accounts = vec![
            (pda, am_account),
            (non_admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
            ))],
        );
    }

    #[test]
    fn test_propose_cpi_rejection() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority,
            },
            build_propose_account_metas(pda, admin),
        );

        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            cpi_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_access_manager_cpi_rejection_error()],
        );
    }

    #[test]
    fn test_propose_zero_address() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();

        let (pda, am_account) = create_initialized_access_manager(admin);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority: Pubkey::default(),
            },
            build_propose_account_metas(pda, admin),
        );

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::ZeroAccount as u32,
            ))],
        );
    }

    #[test]
    fn test_propose_self_transfer() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

        let (pda, am_account) = create_initialized_access_manager(admin);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority: upgrade_authority_pda,
            },
            build_propose_account_metas(pda, admin),
        );

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::SelfTransfer as u32,
            ))],
        );
    }

    #[test]
    fn test_propose_already_pending() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let existing_authority = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) =
            create_access_manager_with_pending(admin, target_program, existing_authority);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority,
            },
            build_propose_account_metas(pda, admin),
        );

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::PendingTransferAlreadyExists as u32,
            ))],
        );
    }

    #[test]
    fn test_propose_fake_sysvar_wormhole_attack() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority,
            },
            build_propose_account_metas(pda, admin),
        );

        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            fake_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    // ── Accept tests ─────────────────────────────────────────────────────────

    #[test]
    #[ignore = "Requires full integration test setup with BPF Loader"]
    fn test_accept_success() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let program_data_address = derive_program_data(&target_program);

        let (pda, am_account) =
            create_access_manager_with_pending(admin, target_program, new_authority);

        let instruction = build_instruction(
            crate::instruction::AcceptUpgradeAuthorityTransfer { target_program },
            build_accept_account_metas(
                pda,
                program_data_address,
                upgrade_authority_pda,
                new_authority,
            ),
        );

        let mut accounts = vec![(pda, am_account)];
        accounts.extend(create_accept_accounts(
            program_data_address,
            upgrade_authority_pda,
            new_authority,
        ));

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
    }

    #[test]
    fn test_accept_wrong_signer() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let pending_authority = Pubkey::new_unique();
        let wrong_signer = Pubkey::new_unique();
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let program_data_address = derive_program_data(&target_program);

        let (pda, am_account) =
            create_access_manager_with_pending(admin, target_program, pending_authority);

        let instruction = build_instruction(
            crate::instruction::AcceptUpgradeAuthorityTransfer { target_program },
            build_accept_account_metas(
                pda,
                program_data_address,
                upgrade_authority_pda,
                wrong_signer,
            ),
        );

        let mut accounts = vec![(pda, am_account)];
        accounts.extend(create_accept_accounts(
            program_data_address,
            upgrade_authority_pda,
            wrong_signer,
        ));

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::AuthorityMismatch as u32,
            ))],
        );
    }

    #[test]
    fn test_accept_no_pending() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let program_data_address = derive_program_data(&target_program);

        let (pda, am_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::AcceptUpgradeAuthorityTransfer { target_program },
            build_accept_account_metas(
                pda,
                program_data_address,
                upgrade_authority_pda,
                new_authority,
            ),
        );

        let mut accounts = vec![(pda, am_account)];
        accounts.extend(create_accept_accounts(
            program_data_address,
            upgrade_authority_pda,
            new_authority,
        ));

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::NoPendingTransfer as u32,
            ))],
        );
    }

    #[test]
    fn test_accept_wrong_target_program() {
        let admin = Pubkey::new_unique();
        let pending_target = Pubkey::new_unique();
        let wrong_target = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&wrong_target, &crate::ID);
        let program_data_address = derive_program_data(&wrong_target);

        let (pda, am_account) =
            create_access_manager_with_pending(admin, pending_target, new_authority);

        let instruction = build_instruction(
            crate::instruction::AcceptUpgradeAuthorityTransfer {
                target_program: wrong_target,
            },
            build_accept_account_metas(
                pda,
                program_data_address,
                upgrade_authority_pda,
                new_authority,
            ),
        );

        let mut accounts = vec![(pda, am_account)];
        accounts.extend(create_accept_accounts(
            program_data_address,
            upgrade_authority_pda,
            new_authority,
        ));

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::PendingTransferMismatch as u32,
            ))],
        );
    }

    // ── Cancel tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_cancel_success() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) =
            create_access_manager_with_pending(admin, target_program, new_authority);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::CancelUpgradeAuthorityTransfer { target_program },
            build_cancel_account_metas(pda, admin),
        );

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        let result =
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

        let am = get_access_manager_from_result(&result, &pda);
        assert_eq!(am.pending_authority_transfer, None);
    }

    #[test]
    fn test_cancel_not_admin() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) =
            create_access_manager_with_pending(admin, target_program, new_authority);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::CancelUpgradeAuthorityTransfer { target_program },
            build_cancel_account_metas(pda, non_admin),
        );

        let accounts = vec![
            (pda, am_account),
            (non_admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
            ))],
        );
    }

    #[test]
    fn test_cancel_no_pending() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();

        let (pda, am_account) = create_initialized_access_manager(admin);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::CancelUpgradeAuthorityTransfer { target_program },
            build_cancel_account_metas(pda, admin),
        );

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::NoPendingTransfer as u32,
            ))],
        );
    }

    #[test]
    fn test_cancel_wrong_target_program() {
        let admin = Pubkey::new_unique();
        let pending_target = Pubkey::new_unique();
        let wrong_target = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) =
            create_access_manager_with_pending(admin, pending_target, new_authority);
        let sysvar_account = create_instructions_sysvar_account();

        let instruction = build_instruction(
            crate::instruction::CancelUpgradeAuthorityTransfer {
                target_program: wrong_target,
            },
            build_cancel_account_metas(pda, admin),
        );

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            (solana_sdk::sysvar::instructions::ID, sysvar_account),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::PendingTransferMismatch as u32,
            ))],
        );
    }

    #[test]
    fn test_cancel_cpi_rejection() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (pda, am_account) =
            create_access_manager_with_pending(admin, target_program, new_authority);

        let instruction = build_instruction(
            crate::instruction::CancelUpgradeAuthorityTransfer { target_program },
            build_cancel_account_metas(pda, admin),
        );

        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (pda, am_account),
            (admin, create_signer_account()),
            cpi_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_access_manager_cpi_rejection_error()],
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::state::AccessManager;
    use crate::test_utils::*;
    use anchor_lang::prelude::bpf_loader_upgradeable;
    use anchor_lang::InstructionData;
    use solana_sdk::{
        account::Account,
        bpf_loader_upgradeable::UpgradeableLoaderState,
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
    };

    fn setup_program_test(
        admin: &Pubkey,
        whitelisted: &[Pubkey],
    ) -> (solana_program_test::ProgramTest, Pubkey) {
        let mut pt = setup_program_test_with_whitelist(admin, whitelisted);

        let target_program = Pubkey::new_unique();
        let (program_data_pda, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

        let pd_account = Account::new_data_with_space(
            10_000_000_000,
            &UpgradeableLoaderState::ProgramData {
                slot: 0,
                upgrade_authority_address: Some(upgrade_authority_pda),
            },
            UpgradeableLoaderState::size_of_programdata_metadata(),
            &bpf_loader_upgradeable::ID,
        )
        .unwrap();
        pt.add_account(program_data_pda, pd_account);

        pt.add_account(
            upgrade_authority_pda,
            Account {
                lamports: 1_000_000,
                owner: solana_sdk::system_program::ID,
                ..Default::default()
            },
        );

        (pt, target_program)
    }

    fn build_propose_ix(
        authority: Pubkey,
        target_program: Pubkey,
        new_authority: Pubkey,
    ) -> Instruction {
        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: crate::instruction::ProposeUpgradeAuthorityTransfer {
                target_program,
                new_authority,
            }
            .data(),
        }
    }

    fn build_accept_ix(target_program: Pubkey, new_authority: Pubkey) -> Instruction {
        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);
        let (program_data, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new(program_data, false),
                AccountMeta::new_readonly(upgrade_authority_pda, false),
                AccountMeta::new_readonly(new_authority, true),
                AccountMeta::new_readonly(bpf_loader_upgradeable::ID, false),
            ],
            data: crate::instruction::AcceptUpgradeAuthorityTransfer { target_program }.data(),
        }
    }

    fn build_cancel_ix(authority: Pubkey, target_program: Pubkey) -> Instruction {
        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: crate::instruction::CancelUpgradeAuthorityTransfer { target_program }.data(),
        }
    }

    async fn get_program_data_authority(
        banks_client: &solana_program_test::BanksClient,
        target_program: Pubkey,
    ) -> Option<Pubkey> {
        let (program_data_pda, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let account = banks_client
            .get_account(program_data_pda)
            .await
            .unwrap()
            .unwrap();
        let state: UpgradeableLoaderState = bincode::deserialize(&account.data).unwrap();
        match state {
            UpgradeableLoaderState::ProgramData {
                upgrade_authority_address,
                ..
            } => upgrade_authority_address,
            _ => panic!("unexpected state"),
        }
    }

    async fn get_pending_transfer(
        banks_client: &solana_program_test::BanksClient,
    ) -> Option<crate::types::PendingAuthorityTransfer> {
        let (pda, _) = Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);
        let account = banks_client.get_account(pda).await.unwrap().unwrap();
        let am: AccessManager =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &account.data[..]).unwrap();
        am.pending_authority_transfer
    }

    #[tokio::test]
    async fn test_propose_and_accept_succeeds() {
        let admin = Keypair::new();
        let new_authority = Keypair::new();
        let (pt, target_program) = setup_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // Propose
        let propose_ix = build_propose_ix(admin.pubkey(), target_program, new_authority.pubkey());
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[propose_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("propose should succeed");

        let pending = get_pending_transfer(&banks_client).await;
        assert_eq!(
            pending,
            Some(crate::types::PendingAuthorityTransfer {
                target_program,
                new_authority: new_authority.pubkey(),
            })
        );

        // Accept
        let accept_ix = build_accept_ix(target_program, new_authority.pubkey());
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[accept_ix],
            Some(&payer.pubkey()),
            &[&payer, &new_authority],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("accept should succeed");

        let authority = get_program_data_authority(&banks_client, target_program).await;
        assert_eq!(
            authority,
            Some(new_authority.pubkey()),
            "upgrade authority should be transferred"
        );

        let pending = get_pending_transfer(&banks_client).await;
        assert_eq!(pending, None, "pending transfer should be cleared");
    }

    #[tokio::test]
    async fn test_propose_cancel_and_repropose() {
        let admin = Keypair::new();
        let first_authority = Keypair::new();
        let second_authority = Keypair::new();
        let (pt, target_program) = setup_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // Propose first authority
        let ix = build_propose_ix(admin.pubkey(), target_program, first_authority.pubkey());
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("first propose should succeed");

        // Cancel
        let ix = build_cancel_ix(admin.pubkey(), target_program);
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("cancel should succeed");

        let pending = get_pending_transfer(&banks_client).await;
        assert_eq!(pending, None, "pending should be cleared after cancel");

        // Re-propose with second authority
        let ix = build_propose_ix(admin.pubkey(), target_program, second_authority.pubkey());
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("second propose should succeed");

        // Accept with second authority
        let ix = build_accept_ix(target_program, second_authority.pubkey());
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &second_authority],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("accept should succeed");

        let authority = get_program_data_authority(&banks_client, target_program).await;
        assert_eq!(authority, Some(second_authority.pubkey()));
    }

    #[tokio::test]
    async fn test_propose_non_admin_rejected() {
        let admin = Keypair::new();
        let non_admin = Keypair::new();
        let new_authority = Pubkey::new_unique();
        let (pt, target_program) = setup_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_propose_ix(non_admin.pubkey(), target_program, new_authority);
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &non_admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::Unauthorized as u32),
        );
    }

    #[tokio::test]
    async fn test_accept_wrong_signer_rejected() {
        let admin = Keypair::new();
        let real_authority = Pubkey::new_unique();
        let wrong_signer = Keypair::new();
        let (pt, target_program) = setup_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // Propose with real_authority
        let ix = build_propose_ix(admin.pubkey(), target_program, real_authority);
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("propose should succeed");

        // Try accept with wrong signer
        let ix = build_accept_ix(target_program, wrong_signer.pubkey());
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &wrong_signer],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::AuthorityMismatch as u32),
        );
    }

    #[tokio::test]
    async fn test_propose_whitelisted_cpi_succeeds() {
        let admin = Keypair::new();
        let new_authority = Keypair::new();
        let (pt, target_program) = setup_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_propose_ix(admin.pubkey(), target_program, new_authority.pubkey());
        let wrapped_ix = wrap_in_test_cpi_target_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("whitelisted CPI propose should succeed");

        let pending = get_pending_transfer(&banks_client).await;
        assert_eq!(
            pending,
            Some(crate::types::PendingAuthorityTransfer {
                target_program,
                new_authority: new_authority.pubkey(),
            })
        );
    }

    #[tokio::test]
    async fn test_propose_unauthorized_cpi_rejected() {
        let admin = Keypair::new();
        let new_authority = Pubkey::new_unique();
        let (pt, target_program) = setup_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_propose_ix(admin.pubkey(), target_program, new_authority);
        let wrapped_ix = wrap_in_test_cpi_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::CpiNotAllowed as u32),
        );
    }

    #[tokio::test]
    async fn test_cancel_unauthorized_cpi_rejected() {
        let admin = Keypair::new();
        let new_authority = Keypair::new();
        let (pt, target_program) = setup_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // First propose successfully
        let ix = build_propose_ix(admin.pubkey(), target_program, new_authority.pubkey());
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("propose should succeed");

        // Try cancel via unauthorized CPI
        let inner_ix = build_cancel_ix(admin.pubkey(), target_program);
        let wrapped_ix = wrap_in_test_cpi_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::CpiNotAllowed as u32),
        );
    }
}
