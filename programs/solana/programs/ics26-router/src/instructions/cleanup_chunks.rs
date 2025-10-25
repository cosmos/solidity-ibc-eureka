use crate::errors::RouterError;
use crate::instructions::cleanup_utils::close_account;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgCleanupChunks)]
pub struct CleanupChunks<'info> {
    /// Relayer who created the chunks and can clean them up
    #[account(mut)]
    pub relayer: Signer<'info>,
}

pub fn cleanup_chunks<'info>(
    ctx: Context<'_, '_, '_, 'info, CleanupChunks<'info>>,
    msg: MsgCleanupChunks,
) -> Result<()> {
    let relayer_key = ctx.accounts.relayer.key();
    let mut chunk_index = 0;

    // Clean payload chunks for each payload
    for (payload_idx, &total_chunks) in msg.payload_chunks.iter().enumerate() {
        for i in 0..total_chunks {
            require!(
                chunk_index < ctx.remaining_accounts.len(),
                RouterError::InvalidChunkCount
            );

            let chunk_account = &ctx.remaining_accounts[chunk_index];

            // Verify the PDA is correct
            let (expected_pda, _) = Pubkey::find_program_address(
                &[
                    PAYLOAD_CHUNK_SEED,
                    relayer_key.as_ref(),
                    msg.client_id.as_bytes(),
                    &msg.sequence.to_le_bytes(),
                    &[payload_idx as u8],
                    &[i],
                ],
                ctx.program_id,
            );

            require!(
                chunk_account.key() == expected_pda,
                RouterError::InvalidChunkAccount
            );

            // Return rent to relayer
            close_account(chunk_account, &ctx.accounts.relayer.to_account_info())?;
            chunk_index += 1;
        }
    }

    // Clean proof chunks
    for i in 0..msg.total_proof_chunks {
        require!(
            chunk_index < ctx.remaining_accounts.len(),
            RouterError::InvalidChunkCount
        );

        let chunk_account = &ctx.remaining_accounts[chunk_index];

        // Verify the PDA is correct
        let (expected_pda, _) = Pubkey::find_program_address(
            &[
                PROOF_CHUNK_SEED,
                relayer_key.as_ref(),
                msg.client_id.as_bytes(),
                &msg.sequence.to_le_bytes(),
                &[i],
            ],
            ctx.program_id,
        );

        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Return rent to relayer
        close_account(chunk_account, &ctx.accounts.relayer.to_account_info())?;
        chunk_index += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;

    struct CleanupChunksTestContext {
        instruction: Instruction,
        accounts: Vec<(Pubkey, Account)>,
        relayer: Pubkey,
        payload_chunk_pubkeys: Vec<Pubkey>,
        proof_chunk_pubkeys: Vec<Pubkey>,
    }

    fn setup_cleanup_chunks_test(
        num_payloads: usize,
        chunks_per_payload: u8,
        num_proof_chunks: u8,
    ) -> CleanupChunksTestContext {
        let relayer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;

        let mut accounts = vec![];
        let mut remaining_account_metas = vec![];
        let mut payload_chunk_pubkeys = vec![];
        let mut proof_chunk_pubkeys = vec![];

        // Create relayer account with initial balance
        accounts.push(create_system_account(relayer));

        // Create payload chunks for each payload
        let mut payload_chunks_count = vec![];
        for payload_idx in 0..num_payloads {
            for chunk_idx in 0..chunks_per_payload {
                let (chunk_pubkey, chunk_account) = create_payload_chunk_account(
                    relayer,
                    client_id,
                    sequence,
                    payload_idx as u8,
                    chunk_idx,
                    vec![0u8; 100],
                );

                // Set lamports to a test value so we can verify cleanup
                let mut modified_account = chunk_account;
                modified_account.lamports = 2_000_000;

                payload_chunk_pubkeys.push(chunk_pubkey);
                accounts.push((chunk_pubkey, modified_account));
                remaining_account_metas.push(AccountMeta::new(chunk_pubkey, false));
            }
            payload_chunks_count.push(chunks_per_payload);
        }

        // Create proof chunks
        for chunk_idx in 0..num_proof_chunks {
            let (chunk_pubkey, chunk_account) =
                create_proof_chunk_account(relayer, client_id, sequence, chunk_idx, vec![0u8; 100]);

            // Set lamports to a test value so we can verify cleanup
            let mut modified_account = chunk_account;
            modified_account.lamports = 2_000_000;

            proof_chunk_pubkeys.push(chunk_pubkey);
            accounts.push((chunk_pubkey, modified_account));
            remaining_account_metas.push(AccountMeta::new(chunk_pubkey, false));
        }

        let msg = MsgCleanupChunks {
            client_id: client_id.to_string(),
            sequence,
            payload_chunks: payload_chunks_count,
            total_proof_chunks: num_proof_chunks,
        };

        let instruction_data = crate::instruction::CleanupChunks { msg };

        let mut account_metas = vec![AccountMeta::new(relayer, true)];
        account_metas.extend(remaining_account_metas);

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: account_metas,
            data: instruction_data.data(),
        };

        CleanupChunksTestContext {
            instruction,
            accounts,
            relayer,
            payload_chunk_pubkeys,
            proof_chunk_pubkeys,
        }
    }

    #[test]
    fn test_cleanup_chunks_success() {
        // Setup with 2 payloads, 3 chunks each, and 2 proof chunks
        let ctx = setup_cleanup_chunks_test(2, 3, 2);

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        // Calculate expected rent reclaimed: (2 payloads * 3 chunks) + 2 proof chunks = 8 chunks total
        let expected_reclaimed = 8 * 2_000_000;
        let initial_relayer_balance = ctx.accounts[0].1.lamports;

        let checks = vec![
            Check::success(),
            // Verify relayer received all the rent back
            Check::account(&ctx.relayer)
                .lamports(initial_relayer_balance + expected_reclaimed)
                .build(),
            // Verify first payload chunk is closed
            Check::account(&ctx.payload_chunk_pubkeys[0])
                .lamports(0)
                .build(),
            // Verify first proof chunk is closed
            Check::account(&ctx.proof_chunk_pubkeys[0])
                .lamports(0)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_invalid_chunk_count() {
        // Setup with 1 payload with 2 chunks, but only provide 1 chunk in remaining_accounts
        let mut ctx = setup_cleanup_chunks_test(1, 2, 0);

        // Remove one chunk account to cause mismatch
        ctx.accounts.pop();
        ctx.instruction.accounts.pop(); // Also remove from instruction

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidChunkCount as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_invalid_payload_chunk_pda() {
        let mut ctx = setup_cleanup_chunks_test(1, 1, 0);

        // Replace the chunk account with wrong pubkey
        let wrong_pubkey = Pubkey::new_unique();
        let chunk_account = ctx.accounts.pop().unwrap().1;
        ctx.accounts.push((wrong_pubkey, chunk_account));

        // Update instruction to reference wrong pubkey
        ctx.instruction.accounts[1].pubkey = wrong_pubkey;

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidChunkAccount as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_invalid_proof_chunk_pda() {
        let mut ctx = setup_cleanup_chunks_test(0, 0, 1);

        // Replace the proof chunk account with wrong pubkey
        let wrong_pubkey = Pubkey::new_unique();
        let chunk_account = ctx.accounts.pop().unwrap().1;
        ctx.accounts.push((wrong_pubkey, chunk_account));

        // Update instruction to reference wrong pubkey
        ctx.instruction.accounts[1].pubkey = wrong_pubkey;

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidChunkAccount as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_multiple_payloads() {
        // Test with more complex scenario: 3 payloads with varying chunk counts + proof chunks
        let ctx = setup_cleanup_chunks_test(3, 2, 3);

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        // 3 payloads * 2 chunks + 3 proof chunks = 9 chunks total
        let expected_reclaimed = 9 * 2_000_000;
        let initial_relayer_balance = ctx.accounts[0].1.lamports;

        let checks = vec![
            Check::success(),
            Check::account(&ctx.relayer)
                .lamports(initial_relayer_balance + expected_reclaimed)
                .build(),
            // Spot check a few chunks are closed
            Check::account(&ctx.payload_chunk_pubkeys[0])
                .lamports(0)
                .build(),
            Check::account(&ctx.payload_chunk_pubkeys[ctx.payload_chunk_pubkeys.len() - 1])
                .lamports(0)
                .build(),
            Check::account(&ctx.proof_chunk_pubkeys[0])
                .lamports(0)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_payload_only() {
        // Test cleanup with only payload chunks, no proof chunks
        let ctx = setup_cleanup_chunks_test(2, 4, 0);

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let expected_reclaimed = 8 * 2_000_000; // 2 payloads * 4 chunks
        let initial_relayer_balance = ctx.accounts[0].1.lamports;

        let checks = vec![
            Check::success(),
            Check::account(&ctx.relayer)
                .lamports(initial_relayer_balance + expected_reclaimed)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_proof_only() {
        // Test cleanup with only proof chunks, no payload chunks
        let ctx = setup_cleanup_chunks_test(0, 0, 5);

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let expected_reclaimed = 5 * 2_000_000;
        let initial_relayer_balance = ctx.accounts[0].1.lamports;

        let checks = vec![
            Check::success(),
            Check::account(&ctx.relayer)
                .lamports(initial_relayer_balance + expected_reclaimed)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }
}
