use crate::constants::{CLEANUP_GRACE_PERIOD, MAX_CLEANUP_BATCH_SIZE};
use crate::errors::RouterError;
use crate::instructions::cleanup_utils::close_account;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CleanupTarget {
    pub client_id: String,
    pub sequence: u64,
    pub created_at: i64, // Timestamp when the packet was processed
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MsgCleanupPacketData {
    pub receipts: Vec<CleanupTarget>,
    pub acks: Vec<CleanupTarget>,
    pub commitments: Vec<CleanupTarget>,
}

#[derive(Accounts)]
#[instruction(msg: MsgCleanupPacketData)]
pub struct CleanupPacketData<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    /// The account that will receive the reclaimed rent
    #[account(mut)]
    pub rent_recipient: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Check if a cleanup target is past the grace period
const fn is_past_grace_period(target: &CleanupTarget, current_timestamp: i64) -> bool {
    let age = current_timestamp.saturating_sub(target.created_at);
    age >= CLEANUP_GRACE_PERIOD as i64
}

/// Check if an account should be skipped (not owned by program or already closed)
fn should_skip_account(account: &AccountInfo, program_id: &Pubkey) -> bool {
    account.owner != program_id || account.lamports() == 0
}

/// Generic function to cleanup a list of targets (receipts, acks, or commitments)
/// Returns (`total_reclaimed`, `cleaned_count`)
#[allow(clippy::too_many_arguments)]
fn cleanup_targets<'info>(
    targets: &[CleanupTarget],
    remaining_accounts: &[AccountInfo<'info>],
    start_idx: usize,
    pda_seed: &[u8],
    rent_recipient: &AccountInfo<'info>,
    current_timestamp: i64,
    program_id: &Pubkey,
    account_type: &str,
) -> Result<(u64, usize)> {
    let mut total_reclaimed = 0u64;
    let mut cleaned_count = 0usize;

    for (idx, target) in targets.iter().enumerate() {
        // Check if enough time has passed since creation
        if !is_past_grace_period(target, current_timestamp) {
            let age = current_timestamp.saturating_sub(target.created_at);
            msg!(
                "Skipping {} {}/{}: too recent (age: {}s < {}s)",
                account_type,
                target.client_id,
                target.sequence,
                age,
                CLEANUP_GRACE_PERIOD
            );
            continue;
        }

        // Get the account from remaining accounts
        let account = remaining_accounts
            .get(start_idx + idx)
            .ok_or(RouterError::MissingAccount)?;

        // Verify PDA address matches expected
        let (expected_pda, _) = Pubkey::find_program_address(
            &[
                pda_seed,
                target.client_id.as_bytes(),
                &target.sequence.to_le_bytes(),
            ],
            program_id,
        );

        require!(
            account.key() == expected_pda,
            RouterError::InvalidAccount
        );

        // Check if account should be skipped
        if should_skip_account(account, program_id) {
            msg!(
                "Skipping {} {}/{}: not owned by program or already closed",
                account_type,
                target.client_id,
                target.sequence
            );
            continue;
        }

        // Close the account and reclaim rent
        let lamports_reclaimed = close_account(account, rent_recipient)?;

        total_reclaimed = total_reclaimed
            .checked_add(lamports_reclaimed)
            .ok_or(RouterError::ArithmeticOverflow)?;
        cleaned_count += 1;

        msg!(
            "Cleaned {} {}/{}, reclaimed {} lamports",
            account_type,
            target.client_id,
            target.sequence,
            lamports_reclaimed
        );
    }

    Ok((total_reclaimed, cleaned_count))
}

pub fn cleanup_packet_data<'info>(
    ctx: Context<'_, '_, '_, 'info, CleanupPacketData<'info>>,
    msg: MsgCleanupPacketData,
) -> Result<()> {
    msg!("=== cleanup_packet_data START ===");

    // Get current timestamp
    let clock = Clock::get()?;
    let current_timestamp = clock.unix_timestamp;

    // Validate batch size
    let total_cleanups = msg.receipts.len() + msg.acks.len() + msg.commitments.len();
    require!(
        total_cleanups <= MAX_CLEANUP_BATCH_SIZE as usize,
        RouterError::ExceedsMaxBatchSize
    );
    require!(total_cleanups > 0, RouterError::EmptyCleanupBatch);

    msg!(
        "Cleaning up {} receipts, {} acks, and {} commitments",
        msg.receipts.len(),
        msg.acks.len(),
        msg.commitments.len()
    );

    let rent_recipient = &ctx.accounts.rent_recipient.to_account_info();

    // Process receipt cleanups
    let (receipts_reclaimed, receipts_cleaned) = cleanup_targets(
        &msg.receipts,
        ctx.remaining_accounts,
        0,
        PACKET_RECEIPT_SEED,
        rent_recipient,
        current_timestamp,
        ctx.program_id,
        "receipt",
    )?;

    // Process ack cleanups
    let ack_start_idx = msg.receipts.len();
    let (acks_reclaimed, acks_cleaned) = cleanup_targets(
        &msg.acks,
        ctx.remaining_accounts,
        ack_start_idx,
        PACKET_ACK_SEED,
        rent_recipient,
        current_timestamp,
        ctx.program_id,
        "ack",
    )?;

    // Process commitment cleanups
    let commitment_start_idx = msg.receipts.len() + msg.acks.len();
    let (commitments_reclaimed, commitments_cleaned) = cleanup_targets(
        &msg.commitments,
        ctx.remaining_accounts,
        commitment_start_idx,
        PACKET_COMMITMENT_SEED,
        rent_recipient,
        current_timestamp,
        ctx.program_id,
        "commitment",
    )?;

    // Calculate totals
    let total_reclaimed = receipts_reclaimed
        .checked_add(acks_reclaimed)
        .and_then(|sum| sum.checked_add(commitments_reclaimed))
        .ok_or(RouterError::ArithmeticOverflow)?;

    let total_cleaned = receipts_cleaned + acks_cleaned + commitments_cleaned;

    msg!(
        "=== cleanup_packet_data SUCCESS: cleaned {} PDAs, reclaimed {} lamports ===",
        total_cleaned,
        total_reclaimed
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{clock::Clock, system_program};

    struct CleanupPacketDataTestContext {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
        receipt_pubkeys: Vec<Pubkey>,
        ack_pubkeys: Vec<Pubkey>,
        commitment_pubkeys: Vec<Pubkey>,
    }

    fn setup_cleanup_test(
        num_receipts: usize,
        num_acks: usize,
        num_commitments: usize,
        created_at: i64,
    ) -> CleanupPacketDataTestContext {
        let authority = Pubkey::new_unique();
        let rent_recipient = Pubkey::new_unique();
        let client_id = "test-client";

        let (router_state_pda, router_state_data) = setup_router_state(authority);

        let mut receipts = Vec::new();
        let mut acks = Vec::new();
        let mut commitments = Vec::new();
        let mut receipt_pubkeys = Vec::new();
        let mut ack_pubkeys = Vec::new();
        let mut commitment_pubkeys = Vec::new();
        let mut remaining_accounts = Vec::new();

        // Create receipt targets and accounts
        for i in 0..num_receipts {
            let sequence = (i + 1) as u64;
            receipts.push(CleanupTarget {
                client_id: client_id.to_string(),
                sequence,
                created_at,
            });

            let (receipt_pda, _) = Pubkey::find_program_address(
                &[
                    PACKET_RECEIPT_SEED,
                    client_id.as_bytes(),
                    &sequence.to_le_bytes(),
                ],
                &crate::ID,
            );
            receipt_pubkeys.push(receipt_pda);

            // Create a commitment account with some lamports (simulating rent)
            let commitment_data = Commitment {
                value: [0u8; 32],
                created_at,
            };
            let mut data = vec![];
            commitment_data.try_serialize(&mut data).unwrap();

            remaining_accounts.push((
                receipt_pda,
                solana_sdk::account::Account {
                    lamports: 2_000_000, // ~0.002 SOL rent
                    data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ));
        }

        // Create ack targets and accounts
        for i in 0..num_acks {
            let sequence = (i + 100) as u64; // Different sequence range
            acks.push(CleanupTarget {
                client_id: client_id.to_string(),
                sequence,
                created_at,
            });

            let (ack_pda, _) = Pubkey::find_program_address(
                &[
                    PACKET_ACK_SEED,
                    client_id.as_bytes(),
                    &sequence.to_le_bytes(),
                ],
                &crate::ID,
            );
            ack_pubkeys.push(ack_pda);

            // Create a commitment account with some lamports (simulating rent)
            let commitment_data = Commitment {
                value: [1u8; 32], // Different value for acks
                created_at,
            };
            let mut data = vec![];
            commitment_data.try_serialize(&mut data).unwrap();

            remaining_accounts.push((
                ack_pda,
                solana_sdk::account::Account {
                    lamports: 2_000_000, // ~0.002 SOL rent
                    data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ));
        }

        // Create commitment targets and accounts
        for i in 0..num_commitments {
            let sequence = (i + 200) as u64; // Different sequence range
            commitments.push(CleanupTarget {
                client_id: client_id.to_string(),
                sequence,
                created_at,
            });

            let (commitment_pda, _) = Pubkey::find_program_address(
                &[
                    PACKET_COMMITMENT_SEED,
                    client_id.as_bytes(),
                    &sequence.to_le_bytes(),
                ],
                &crate::ID,
            );
            commitment_pubkeys.push(commitment_pda);

            // Create a commitment account with some lamports (simulating rent)
            let commitment_data = Commitment {
                value: [2u8; 32], // Different value for commitments
                created_at,       // Store the timestamp
            };
            let mut data = vec![];
            commitment_data.try_serialize(&mut data).unwrap();

            remaining_accounts.push((
                commitment_pda,
                solana_sdk::account::Account {
                    lamports: 2_000_000, // ~0.002 SOL rent
                    data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ));
        }

        let msg = MsgCleanupPacketData {
            receipts,
            acks,
            commitments,
        };

        let mut instruction_accounts = vec![
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new(rent_recipient, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        // Add remaining accounts as mutable (to be closed)
        for (pubkey, _) in &remaining_accounts {
            instruction_accounts.push(AccountMeta::new(*pubkey, false));
        }

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: instruction_accounts,
            data: crate::instruction::CleanupPacketData { msg }.data(),
        };

        let mut accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_system_account(rent_recipient),
            create_program_account(system_program::ID),
        ];

        // Add remaining accounts
        accounts.extend(remaining_accounts);

        CleanupPacketDataTestContext {
            instruction,
            accounts,
            receipt_pubkeys,
            ack_pubkeys,
            commitment_pubkeys,
        }
    }

    fn create_clock_data(timestamp: i64) -> Vec<u8> {
        let mut clock_data = vec![0u8; Clock::size_of()];
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: timestamp,
        };
        bincode::serialize_into(&mut clock_data[..], &clock).unwrap();
        clock_data
    }

    #[test]
    fn test_cleanup_packet_data_success() {
        let created_at = 1000;
        let current_time = created_at + CLEANUP_GRACE_PERIOD as i64 + 100; // Past grace period

        let mut ctx = setup_cleanup_test(2, 2, 2, created_at);

        // Add clock sysvar
        ctx.accounts
            .push(create_clock_account_with_data(create_clock_data(
                current_time,
            )));

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        // Calculate expected rent reclaimed (2 receipts + 2 acks + 2 commitments) * 2_000_000 lamports
        let expected_reclaimed = 6 * 2_000_000;
        let rent_recipient_pubkey = ctx.accounts[1].0;
        let initial_balance = ctx.accounts[1].1.lamports;

        let checks = vec![
            Check::success(),
            // Verify rent recipient received the rent
            Check::account(&rent_recipient_pubkey)
                .lamports(initial_balance + expected_reclaimed)
                .build(),
            // Verify PDAs are closed (0 lamports)
            Check::account(&ctx.receipt_pubkeys[0]).lamports(0).build(),
            Check::account(&ctx.ack_pubkeys[0]).lamports(0).build(),
            Check::account(&ctx.commitment_pubkeys[0])
                .lamports(0)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_packet_data_grace_period_not_met() {
        let created_at = 1000;
        let current_time = created_at + CLEANUP_GRACE_PERIOD as i64 - 100; // Still within grace period

        let mut ctx = setup_cleanup_test(1, 1, 1, created_at);

        // Add clock sysvar
        ctx.accounts
            .push(create_clock_account_with_data(create_clock_data(
                current_time,
            )));

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let rent_recipient_pubkey = ctx.accounts[1].0;
        let initial_balance = ctx.accounts[1].1.lamports;

        let checks = vec![
            Check::success(),
            // Verify no rent was reclaimed (PDAs too recent)
            Check::account(&rent_recipient_pubkey)
                .lamports(initial_balance) // No change
                .build(),
            // Verify PDAs still have lamports (not closed)
            Check::account(&ctx.receipt_pubkeys[0])
                .lamports(2_000_000)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_packet_data_batch_size_exceeded() {
        // Try to cleanup more than MAX_CLEANUP_BATCH_SIZE
        let ctx = setup_cleanup_test(
            MAX_CLEANUP_BATCH_SIZE as usize / 3 + 1,
            MAX_CLEANUP_BATCH_SIZE as usize / 3 + 1,
            MAX_CLEANUP_BATCH_SIZE as usize / 3 + 1,
            0,
        );

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ExceedsMaxBatchSize as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_packet_data_empty_batch() {
        let ctx = setup_cleanup_test(0, 0, 0, 0);

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::EmptyCleanupBatch as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }
}

