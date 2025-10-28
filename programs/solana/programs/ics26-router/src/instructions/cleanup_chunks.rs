use crate::errors::RouterError;
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
            let expected_seeds = &[
                PayloadChunk::SEED,
                relayer_key.as_ref(),
                msg.client_id.as_bytes(),
                &msg.sequence.to_le_bytes(),
                &[payload_idx as u8],
                &[i],
            ];
            let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, ctx.program_id);

            require!(
                chunk_account.key() == expected_pda,
                RouterError::InvalidChunkAccount
            );

            // Return rent to relayer
            cleanup_single_chunk(chunk_account, &ctx.accounts.relayer)?;
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
        let expected_seeds = &[
            ProofChunk::SEED,
            relayer_key.as_ref(),
            msg.client_id.as_bytes(),
            &msg.sequence.to_le_bytes(),
            &[i],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, ctx.program_id);

        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Return rent to relayer
        cleanup_single_chunk(chunk_account, &ctx.accounts.relayer)?;
        chunk_index += 1;
    }

    Ok(())
}

fn cleanup_single_chunk<'info>(
    chunk_account: &AccountInfo<'info>,
    relayer: &Signer<'info>,
) -> Result<()> {
    let mut chunk_lamports = chunk_account.try_borrow_mut_lamports()?;
    let mut relayer_lamports = relayer.try_borrow_mut_lamports()?;

    **relayer_lamports = relayer_lamports
        .checked_add(**chunk_lamports)
        .ok_or(RouterError::ArithmeticOverflow)?;
    **chunk_lamports = 0;

    let mut data = chunk_account.try_borrow_mut_data()?;
    data.fill(0);

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
    use solana_sdk::pubkey::Pubkey;

    #[test]
    fn test_cleanup_chunks_success() {
        let relayer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;

        // Create one payload chunk and one proof chunk
        let (payload_chunk_pda, _) = Pubkey::find_program_address(
            &[
                PayloadChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8], // payload_idx
                &[0u8], // chunk_idx
            ],
            &crate::ID,
        );

        let (proof_chunk_pda, _) = Pubkey::find_program_address(
            &[
                ProofChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8], // chunk_idx
            ],
            &crate::ID,
        );

        let msg = MsgCleanupChunks {
            client_id: client_id.to_string(),
            sequence,
            payload_chunks: vec![1], // 1 chunk for payload 0
            total_proof_chunks: 1,
        };

        let instruction_data = crate::instruction::CleanupChunks { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(relayer, true),
                // Remaining accounts: payload chunks, then proof chunks
                AccountMeta::new(payload_chunk_pda, false),
                AccountMeta::new(proof_chunk_pda, false),
            ],
            data: instruction_data.data(),
        };

        let chunk_rent = 1_000_000; // 1 SOL rent per chunk
        let relayer_initial = 5_000_000; // 5 SOL

        let accounts = vec![
            create_system_account_with_lamports(relayer, relayer_initial),
            create_account_with_lamports(payload_chunk_pda, &crate::ID, chunk_rent, 1000),
            create_account_with_lamports(proof_chunk_pda, &crate::ID, chunk_rent, 1000),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![
            Check::success(),
            // Verify relayer received rent back (initial + 2 chunks worth)
            Check::account(&relayer)
                .lamports(relayer_initial + chunk_rent * 2)
                .build(),
            // Verify chunks are closed (0 lamports)
            Check::account(&payload_chunk_pda).lamports(0).build(),
            Check::account(&proof_chunk_pda).lamports(0).build(),
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_wrong_pda() {
        let relayer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;

        // Create a chunk with wrong PDA (not matching the expected seeds)
        let wrong_chunk = Pubkey::new_unique();

        let msg = MsgCleanupChunks {
            client_id: client_id.to_string(),
            sequence,
            payload_chunks: vec![1],
            total_proof_chunks: 0,
        };

        let instruction_data = crate::instruction::CleanupChunks { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(relayer, true),
                AccountMeta::new(wrong_chunk, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(relayer),
            create_account_with_lamports(wrong_chunk, &crate::ID, 1_000_000, 1000),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(
            anchor_lang::error::Error::from(RouterError::InvalidChunkAccount).into(),
        )];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_cleanup_chunks_wrong_chunk_count() {
        let relayer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;

        // Message says 2 chunks but we only provide 1 account
        let (payload_chunk_pda, _) = Pubkey::find_program_address(
            &[
                PayloadChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8],
                &[0u8],
            ],
            &crate::ID,
        );

        let msg = MsgCleanupChunks {
            client_id: client_id.to_string(),
            sequence,
            payload_chunks: vec![2], // Expecting 2 chunks but we'll only provide 1
            total_proof_chunks: 0,
        };

        let instruction_data = crate::instruction::CleanupChunks { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![AccountMeta::new(relayer, true)],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(relayer),
            create_account_with_lamports(payload_chunk_pda, &crate::ID, 1_000_000, 1000),
            // Missing second chunk!
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(
            anchor_lang::error::Error::from(RouterError::InvalidChunkCount).into(),
        )];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_cleanup_multiple_payloads() {
        let relayer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;

        // Create 2 chunks for payload 0 and 1 chunk for payload 1
        let (payload_0_chunk_0, _) = Pubkey::find_program_address(
            &[
                PayloadChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8],
                &[0u8],
            ],
            &crate::ID,
        );

        let (payload_0_chunk_1, _) = Pubkey::find_program_address(
            &[
                PayloadChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8],
                &[1u8],
            ],
            &crate::ID,
        );

        let (payload_1_chunk_0, _) = Pubkey::find_program_address(
            &[
                PayloadChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[1u8],
                &[0u8],
            ],
            &crate::ID,
        );

        let msg = MsgCleanupChunks {
            client_id: client_id.to_string(),
            sequence,
            payload_chunks: vec![2, 1], // payload 0: 2 chunks, payload 1: 1 chunk
            total_proof_chunks: 0,
        };

        let instruction_data = crate::instruction::CleanupChunks { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(relayer, true),
                // Payload 0 chunks
                AccountMeta::new(payload_0_chunk_0, false),
                AccountMeta::new(payload_0_chunk_1, false),
                // Payload 1 chunks
                AccountMeta::new(payload_1_chunk_0, false),
            ],
            data: instruction_data.data(),
        };

        let chunk_rent = 1_000_000;
        let relayer_initial = 5_000_000;

        let accounts = vec![
            create_system_account_with_lamports(relayer, relayer_initial),
            create_account_with_lamports(payload_0_chunk_0, &crate::ID, chunk_rent, 1000),
            create_account_with_lamports(payload_0_chunk_1, &crate::ID, chunk_rent, 1000),
            create_account_with_lamports(payload_1_chunk_0, &crate::ID, chunk_rent, 1000),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&relayer)
                .lamports(relayer_initial + chunk_rent * 3)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_cleanup_only_proof_chunks() {
        let relayer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;

        // Create 2 proof chunks, no payload chunks
        let (proof_chunk_0, _) = Pubkey::find_program_address(
            &[
                ProofChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8],
            ],
            &crate::ID,
        );

        let (proof_chunk_1, _) = Pubkey::find_program_address(
            &[
                ProofChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[1u8],
            ],
            &crate::ID,
        );

        let msg = MsgCleanupChunks {
            client_id: client_id.to_string(),
            sequence,
            payload_chunks: vec![], // No payload chunks
            total_proof_chunks: 2,
        };

        let instruction_data = crate::instruction::CleanupChunks { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(relayer, true),
                // Proof chunks
                AccountMeta::new(proof_chunk_0, false),
                AccountMeta::new(proof_chunk_1, false),
            ],
            data: instruction_data.data(),
        };

        let chunk_rent = 1_000_000;
        let relayer_initial = 5_000_000;

        let accounts = vec![
            create_system_account_with_lamports(relayer, relayer_initial),
            create_account_with_lamports(proof_chunk_0, &crate::ID, chunk_rent, 1000),
            create_account_with_lamports(proof_chunk_1, &crate::ID, chunk_rent, 1000),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&relayer)
                .lamports(relayer_initial + chunk_rent * 2)
                .build(),
            Check::account(&proof_chunk_0).lamports(0).build(),
            Check::account(&proof_chunk_1).lamports(0).build(),
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
