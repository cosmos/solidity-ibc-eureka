use crate::events::UpgradeAuthorityClaimedEvent;
use crate::helpers::cpi;
use crate::state::AccessManager;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;

/// Claims upgrade authority from a source access manager that has proposed
/// a transfer to this access manager's upgrade authority PDA.
///
/// This enables AM-to-AM migration: the source AM proposes, and the
/// destination AM claims by calling (via CPI) the source's
/// `accept_upgrade_authority_transfer` with its own PDA as signer.
///
/// No admin authorization required -- PDA signing IS the authorization.
/// Only this program can sign with its upgrade authority PDA.
/// The source AM's accept instruction validates the pending transfer matches.
#[derive(Accounts)]
#[instruction(target_program: Pubkey)]
pub struct ClaimUpgradeAuthority<'info> {
    /// This access manager's upgrade authority PDA for the target program.
    /// Signs the CPI as the new authority.
    /// CHECK: Validated via seeds constraint
    #[account(
        seeds = [AccessManager::UPGRADE_AUTHORITY_SEED, target_program.as_ref()],
        bump
    )]
    pub our_upgrade_authority: AccountInfo<'info>,

    /// The source access manager's state PDA.
    /// CHECK: Validated via seeds constraint against `source_access_manager_program`
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump,
        seeds::program = source_access_manager_program.key()
    )]
    pub source_access_manager_state: AccountInfo<'info>,

    /// The target program's data account (BPF Loader Upgradeable PDA).
    /// CHECK: Validated via seeds constraint against BPF Loader Upgradeable
    #[account(
        mut,
        seeds = [target_program.as_ref()],
        bump,
        seeds::program = bpf_loader_upgradeable::ID
    )]
    pub target_program_data: AccountInfo<'info>,

    /// The source access manager's upgrade authority PDA for the target program.
    /// CHECK: Validated via seeds constraint against `source_access_manager_program`
    #[account(
        seeds = [AccessManager::UPGRADE_AUTHORITY_SEED, target_program.as_ref()],
        bump,
        seeds::program = source_access_manager_program.key()
    )]
    pub source_upgrade_authority: AccountInfo<'info>,

    /// The source access manager program (CPI target).
    /// CHECK: Must be executable
    #[account(executable)]
    pub source_access_manager_program: AccountInfo<'info>,

    /// CHECK: Must be BPF Loader Upgradeable program ID
    #[account(address = bpf_loader_upgradeable::ID)]
    pub bpf_loader_upgradeable: AccountInfo<'info>,
}

pub fn claim_upgrade_authority(
    ctx: Context<ClaimUpgradeAuthority>,
    target_program: Pubkey,
) -> Result<()> {
    let cpi_accounts = cpi::AcceptUpgradeAuthorityTransferCpi {
        access_manager: ctx.accounts.source_access_manager_state.to_account_info(),
        program_data: ctx.accounts.target_program_data.to_account_info(),
        upgrade_authority: ctx.accounts.source_upgrade_authority.to_account_info(),
        new_authority: ctx.accounts.our_upgrade_authority.to_account_info(),
        bpf_loader_upgradeable: ctx.accounts.bpf_loader_upgradeable.to_account_info(),
    };
    let bump = [ctx.bumps.our_upgrade_authority];
    let signer_seeds = &[&[
        AccessManager::UPGRADE_AUTHORITY_SEED,
        target_program.as_ref(),
        &bump,
    ][..]];
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.source_access_manager_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    cpi::accept_upgrade_authority_transfer(cpi_ctx, target_program)?;

    emit!(UpgradeAuthorityClaimedEvent {
        program: target_program,
        source_access_manager: ctx.accounts.source_access_manager_program.key(),
        new_authority: ctx.accounts.our_upgrade_authority.key(),
    });

    Ok(())
}

#[cfg(test)]
mod integration_tests {
    use crate::state::AccessManager;
    use crate::test_utils::*;
    use crate::types::{PendingAuthorityTransfer, RoleData};
    use anchor_lang::prelude::bpf_loader_upgradeable;
    use anchor_lang::{AnchorSerialize, Discriminator, InstructionData, Space};
    use solana_ibc_types::roles;
    use solana_sdk::{
        account::Account,
        bpf_loader_upgradeable::UpgradeableLoaderState,
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signer::Signer,
    };

    /// Set up `ProgramTest` with both AM binaries loaded.
    ///
    /// - `crate::ID` is the claimer (destination AM)
    /// - `crate::test_config::OTHER_AM_ID` is the source AM
    ///
    /// `target_program` identifies the program whose upgrade authority is being migrated.
    /// `pending_transfers` sets up the source AM's state with pending transfers.
    fn setup_claim_test(
        target_program: Pubkey,
        pending_transfers: Vec<PendingAuthorityTransfer>,
    ) -> solana_program_test::ProgramTest {
        if std::env::var("SBF_OUT_DIR").is_err() {
            std::env::set_var("SBF_OUT_DIR", std::path::Path::new("../../target/deploy"));
        }

        let mut pt = solana_program_test::ProgramTest::new(
            crate::test_config::PROGRAM_BINARY_NAME,
            crate::ID,
            None,
        );
        pt.add_program(
            crate::test_config::OTHER_AM_BINARY_NAME,
            crate::test_config::OTHER_AM_ID,
            None,
        );

        // Source AM's upgrade authority PDA (current authority of target program)
        let (source_upgrade_authority, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::test_config::OTHER_AM_ID);

        // Target program's program_data with source AM as authority
        let (program_data_pda, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let pd_account = Account::new_data_with_space(
            10_000_000_000,
            &UpgradeableLoaderState::ProgramData {
                slot: 0,
                upgrade_authority_address: Some(source_upgrade_authority),
            },
            UpgradeableLoaderState::size_of_programdata_metadata(),
            &bpf_loader_upgradeable::ID,
        )
        .unwrap();
        pt.add_account(program_data_pda, pd_account);

        // Source AM's upgrade authority PDA account
        pt.add_account(
            source_upgrade_authority,
            Account {
                lamports: 1_000_000,
                owner: solana_sdk::system_program::ID,
                ..Default::default()
            },
        );

        // Source AM's state PDA
        let (source_am_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::test_config::OTHER_AM_ID);
        let source_am = AccessManager {
            roles: vec![RoleData {
                role_id: roles::ADMIN_ROLE,
                members: vec![Pubkey::new_unique()],
            }],
            whitelisted_programs: vec![],
            pending_authority_transfers: pending_transfers,
        };
        let mut am_data = vec![0u8; 8 + AccessManager::INIT_SPACE];
        am_data[0..8].copy_from_slice(AccessManager::DISCRIMINATOR);
        source_am.serialize(&mut &mut am_data[8..]).unwrap();
        pt.add_account(
            source_am_pda,
            Account {
                lamports: 10_000_000,
                data: am_data,
                owner: crate::test_config::OTHER_AM_ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Our (claimer's) upgrade authority PDA account
        let (our_upgrade_authority, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        pt.add_account(
            our_upgrade_authority,
            Account {
                lamports: 1_000_000,
                owner: solana_sdk::system_program::ID,
                ..Default::default()
            },
        );

        pt
    }

    fn build_claim_ix(target_program: Pubkey) -> Instruction {
        let (our_upgrade_authority, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let (source_am_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::test_config::OTHER_AM_ID);
        let (program_data_pda, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let (source_upgrade_authority, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::test_config::OTHER_AM_ID);

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(our_upgrade_authority, false),
                AccountMeta::new(source_am_pda, false),
                AccountMeta::new(program_data_pda, false),
                AccountMeta::new_readonly(source_upgrade_authority, false),
                AccountMeta::new_readonly(crate::test_config::OTHER_AM_ID, false),
                AccountMeta::new_readonly(bpf_loader_upgradeable::ID, false),
            ],
            data: crate::instruction::ClaimUpgradeAuthority { target_program }.data(),
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

    async fn get_source_pending_transfers(
        banks_client: &solana_program_test::BanksClient,
    ) -> Vec<PendingAuthorityTransfer> {
        let (pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::test_config::OTHER_AM_ID);
        let account = banks_client.get_account(pda).await.unwrap().unwrap();
        let am: AccessManager =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &account.data[..]).unwrap();
        am.pending_authority_transfers
    }

    #[tokio::test]
    async fn test_claim_succeeds() {
        let target_program = Pubkey::new_unique();
        let (our_pda, _) = AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

        let pt = setup_claim_test(
            target_program,
            vec![PendingAuthorityTransfer {
                target_program,
                new_authority: our_pda,
            }],
        );
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_claim_ix(target_program);
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );
        banks_client
            .process_transaction(tx)
            .await
            .expect("claim should succeed");

        let authority = get_program_data_authority(&banks_client, target_program).await;
        assert_eq!(
            authority,
            Some(our_pda),
            "upgrade authority should transfer to claimer's PDA"
        );

        let pending = get_source_pending_transfers(&banks_client).await;
        assert!(pending.is_empty(), "pending transfers should be cleared");
    }

    #[tokio::test]
    async fn test_claim_no_pending_fails() {
        let target_program = Pubkey::new_unique();

        let pt = setup_claim_test(target_program, vec![]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_claim_ix(target_program);
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::NoPendingTransfer as u32),
        );
    }

    #[tokio::test]
    async fn test_claim_authority_mismatch_fails() {
        let target_program = Pubkey::new_unique();
        let wrong_authority = Pubkey::new_unique();

        let pt = setup_claim_test(
            target_program,
            vec![PendingAuthorityTransfer {
                target_program,
                new_authority: wrong_authority,
            }],
        );
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_claim_ix(target_program);
        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::AuthorityMismatch as u32),
        );
    }
}
