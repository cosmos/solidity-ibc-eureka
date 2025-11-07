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
            let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, &crate::ID);

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
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, &crate::ID);

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
    {
        let mut data = chunk_account.try_borrow_mut_data()?;
        data.fill(0);
    }

    let mut chunk_lamports = chunk_account.try_borrow_mut_lamports()?;
    let mut relayer_lamports = relayer.try_borrow_mut_lamports()?;

    **relayer_lamports = relayer_lamports
        .checked_add(**chunk_lamports)
        .ok_or(RouterError::ArithmeticOverflow)?;
    **chunk_lamports = 0;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    #[test]
    fn test_cleanup_chunks_success() {
        let relayer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 42u64;

        // Derive chunk PDAs
        let payload_chunk_pda = Pubkey::find_program_address(
            &[
                PayloadChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8], // payload_idx
                &[0u8], // chunk_idx
            ],
            &crate::ID,
        )
        .0;

        let proof_chunk_pda = Pubkey::find_program_address(
            &[
                ProofChunk::SEED,
                relayer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8], // chunk_idx
            ],
            &crate::ID,
        )
        .0;

        // Create chunk accounts with some data
        let payload_chunk_data = vec![1u8; 100];
        let proof_chunk_data = vec![2u8; 100];
        let chunk_rent = 1_500_000u64;
        let initial_relayer_balance = 10_000_000_000u64;

        let instruction_data = crate::instruction::CleanupChunks {
            msg: MsgCleanupChunks {
                client_id: client_id.to_string(),
                sequence,
                payload_chunks: vec![1], // 1 chunk for payload 0
                total_proof_chunks: 1,
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(relayer, true),
                // Remaining accounts: chunks
                AccountMeta::new(payload_chunk_pda, false),
                AccountMeta::new(proof_chunk_pda, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            (
                relayer,
                Account {
                    lamports: initial_relayer_balance,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payload_chunk_pda,
                Account {
                    lamports: chunk_rent,
                    data: payload_chunk_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                proof_chunk_pda,
                Account {
                    lamports: chunk_rent,
                    data: proof_chunk_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
        ];

        let mollusk = Mollusk::new(&crate::ID, get_router_program_path());
        let result = mollusk.process_instruction(&instruction, &accounts);

        // Check success
        assert!(
            matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            ),
            "Instruction should succeed"
        );

        // Verify relayer received rent back
        let relayer_account = result
            .get_account(&relayer)
            .expect("relayer account should exist");
        assert_eq!(
            relayer_account.lamports,
            initial_relayer_balance + (chunk_rent * 2),
            "relayer should receive rent from both chunks"
        );

        // Verify chunks are closed (lamports = 0 and data zeroed)
        let payload_chunk_account = result
            .get_account(&payload_chunk_pda)
            .expect("payload chunk account should exist");
        assert_eq!(
            payload_chunk_account.lamports, 0,
            "payload chunk should be closed"
        );
        assert!(
            payload_chunk_account.data.iter().all(|&b| b == 0),
            "payload chunk data should be zeroed"
        );

        let proof_chunk_account = result
            .get_account(&proof_chunk_pda)
            .expect("proof chunk account should exist");
        assert_eq!(
            proof_chunk_account.lamports, 0,
            "proof chunk should be closed"
        );
        assert!(
            proof_chunk_account.data.iter().all(|&b| b == 0),
            "proof chunk data should be zeroed"
        );
    }
}
