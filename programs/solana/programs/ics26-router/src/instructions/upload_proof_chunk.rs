use crate::errors::RouterError;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgUploadChunk)]
pub struct UploadProofChunk<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ProofChunk::INIT_SPACE,
        seeds = [
            PROOF_CHUNK_SEED,
            payer.key().as_ref(),
            msg.client_id.as_bytes(),
            &msg.sequence.to_le_bytes(),
            &[msg.chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, ProofChunk>,

    #[account(mut)]
    pub payer: Signer<'info>,

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    #[test]
    fn test_upload_proof_chunk_success() {
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;
        let chunk_index = 0u8;
        let chunk_data = vec![0x42u8; 100]; // 100 bytes of data

        let (chunk_pda, _) = Pubkey::find_program_address(
            &[
                PROOF_CHUNK_SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[chunk_index],
            ],
            &crate::ID,
        );

        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0, // Not used for proof chunks
            chunk_index,
            chunk_data: chunk_data.clone(),
        };

        let instruction_data = crate::instruction::UploadProofChunk { msg };

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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&chunk_pda).owner(&crate::ID).build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        // Verify the chunk data was stored correctly
        let chunk_account_data = get_account_data_from_mollusk(&result, &chunk_pda)
            .expect("Chunk account not found");

        let stored_chunk: ProofChunk =
            anchor_lang::AnchorDeserialize::deserialize(&mut &chunk_account_data[..])
                .expect("Failed to deserialize chunk");

        assert_eq!(stored_chunk.client_id, client_id);
        assert_eq!(stored_chunk.sequence, sequence);
        assert_eq!(stored_chunk.chunk_index, chunk_index);
        assert_eq!(stored_chunk.chunk_data, chunk_data);
    }

    #[test]
    fn test_upload_proof_chunk_data_too_large() {
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;
        let chunk_index = 0u8;
        // Create data larger than CHUNK_DATA_SIZE (700 bytes)
        let chunk_data = vec![0x42u8; CHUNK_DATA_SIZE + 1];

        let (chunk_pda, _) = Pubkey::find_program_address(
            &[
                PROOF_CHUNK_SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[chunk_index],
            ],
            &crate::ID,
        );

        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0,
            chunk_index,
            chunk_data,
        };

        let instruction_data = crate::instruction::UploadProofChunk { msg };

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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ChunkDataTooLarge as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_upload_proof_chunk_max_size() {
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;
        let chunk_index = 0u8;
        // Create data exactly CHUNK_DATA_SIZE (700 bytes) - should succeed
        let chunk_data = vec![0x42u8; CHUNK_DATA_SIZE];

        let (chunk_pda, _) = Pubkey::find_program_address(
            &[
                PROOF_CHUNK_SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[chunk_index],
            ],
            &crate::ID,
        );

        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0,
            chunk_index,
            chunk_data,
        };

        let instruction_data = crate::instruction::UploadProofChunk { msg };

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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::success()];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_upload_proof_chunk_multiple_chunks() {
        // Test uploading multiple proof chunks for same packet
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        // Upload chunk 0
        let chunk_data_0 = vec![0x01u8; 100];
        let (chunk_pda_0, _) = Pubkey::find_program_address(
            &[
                PROOF_CHUNK_SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[0u8],
            ],
            &crate::ID,
        );

        let msg_0 = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0,
            chunk_index: 0,
            chunk_data: chunk_data_0,
        };

        let instruction_0 = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(chunk_pda_0, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: crate::instruction::UploadProofChunk { msg: msg_0 }.data(),
        };

        let accounts_0 = vec![
            create_uninitialized_account(chunk_pda_0, 0),
            create_system_account(payer),
            create_program_account(system_program::ID),
        ];

        mollusk.process_and_validate_instruction(
            &instruction_0,
            &accounts_0,
            &[Check::success()],
        );

        // Upload chunk 1
        let chunk_data_1 = vec![0x02u8; 100];
        let (chunk_pda_1, _) = Pubkey::find_program_address(
            &[
                PROOF_CHUNK_SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[1u8],
            ],
            &crate::ID,
        );

        let msg_1 = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0,
            chunk_index: 1,
            chunk_data: chunk_data_1,
        };

        let instruction_1 = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(chunk_pda_1, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: crate::instruction::UploadProofChunk { msg: msg_1 }.data(),
        };

        let accounts_1 = vec![
            create_uninitialized_account(chunk_pda_1, 0),
            create_system_account(payer),
            create_program_account(system_program::ID),
        ];

        mollusk.process_and_validate_instruction(
            &instruction_1,
            &accounts_1,
            &[Check::success()],
        );
    }

    #[test]
    fn test_upload_proof_chunk_empty_data() {
        // Test uploading chunk with empty data - should succeed
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let sequence = 1u64;
        let chunk_index = 0u8;
        let chunk_data = vec![]; // Empty data

        let (chunk_pda, _) = Pubkey::find_program_address(
            &[
                PROOF_CHUNK_SEED,
                payer.as_ref(),
                client_id.as_bytes(),
                &sequence.to_le_bytes(),
                &[chunk_index],
            ],
            &crate::ID,
        );

        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0,
            chunk_index,
            chunk_data,
        };

        let instruction_data = crate::instruction::UploadProofChunk { msg };

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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::success()];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        // Verify empty chunk was stored
        let chunk_account_data = get_account_data_from_mollusk(&result, &chunk_pda)
            .expect("Chunk account not found");

        let stored_chunk: ProofChunk =
            anchor_lang::AnchorDeserialize::deserialize(&mut &chunk_account_data[..])
                .expect("Failed to deserialize chunk");

        assert_eq!(stored_chunk.chunk_data.len(), 0);
    }
}
