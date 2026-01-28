use crate::errors::RouterError;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgUploadChunk)]
pub struct UploadProofChunk<'info> {
    #[account(
        init,
        payer = relayer,
        space = 8 + ProofChunk::INIT_SPACE,
        seeds = [
            ProofChunk::SEED,
            relayer.key().as_ref(),
            msg.client_id.as_bytes(),
            &msg.sequence.to_le_bytes(),
            &[msg.chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, ProofChunk>,

    #[account(mut)]
    pub relayer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn upload_proof_chunk(ctx: Context<UploadProofChunk>, msg: MsgUploadChunk) -> Result<()> {
    let chunk = &mut ctx.accounts.chunk;

    require!(
        msg.chunk_data.len() <= CHUNK_DATA_SIZE,
        RouterError::ChunkDataTooLarge
    );

    chunk.client_id = msg.client_id;
    chunk.sequence = msg.sequence;
    chunk.chunk_index = msg.chunk_index;
    chunk.chunk_data = msg.chunk_data;

    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    #[test]
    fn test_upload_proof_chunk_success() {
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 42u64;
        let chunk_index = 0u8;
        let chunk_data = vec![2u8; 200];

        // Derive chunk PDA
        let chunk_pda = Pubkey::find_program_address(
            &[
                ProofChunk::SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[chunk_index],
            ],
            &crate::ID,
        )
        .0;

        let instruction_data = crate::instruction::UploadProofChunk {
            msg: MsgUploadChunk {
                client_id: client_id.to_string(),
                sequence,
                payload_index: 0, // Not used for proof chunks
                chunk_index,
                chunk_data: chunk_data.clone(),
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(chunk_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_uninitialized_account(chunk_pda, 0),
            create_system_account(payer),
            create_program_account(system_program::ID),
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

        // Verify chunk account was created and initialized
        let chunk_account = result
            .get_account(&chunk_pda)
            .expect("chunk account should exist");
        assert_eq!(
            chunk_account.owner,
            crate::ID,
            "chunk should be owned by program"
        );
        assert!(
            chunk_account.lamports > 0,
            "chunk should have lamports for rent"
        );

        // Deserialize and verify chunk data
        let chunk_data_raw = &chunk_account.data[8..]; // Skip discriminator
        let chunk: ProofChunk = AnchorDeserialize::deserialize(&mut &chunk_data_raw[..])
            .expect("should deserialize chunk");
        assert_eq!(chunk.client_id, client_id);
        assert_eq!(chunk.sequence, sequence);
        assert_eq!(chunk.chunk_index, chunk_index);
        assert_eq!(chunk.chunk_data, chunk_data);
    }

    #[test]
    fn test_upload_proof_chunk_data_too_large() {
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 42u64;
        let chunk_index = 0u8;
        let chunk_data = vec![2u8; CHUNK_DATA_SIZE + 1]; // Too large

        // Derive chunk PDA
        let chunk_pda = Pubkey::find_program_address(
            &[
                ProofChunk::SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[chunk_index],
            ],
            &crate::ID,
        )
        .0;

        let instruction_data = crate::instruction::UploadProofChunk {
            msg: MsgUploadChunk {
                client_id: client_id.to_string(),
                sequence,
                payload_index: 0,
                chunk_index,
                chunk_data,
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(chunk_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_uninitialized_account(chunk_pda, 0),
            create_system_account(payer),
            create_program_account(system_program::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, get_router_program_path());
        let result = mollusk.process_instruction(&instruction, &accounts);

        // Should fail with ChunkDataTooLarge error
        assert_error_code(
            result,
            RouterError::ChunkDataTooLarge,
            "upload_proof_chunk_data_too_large",
        );
    }
}
