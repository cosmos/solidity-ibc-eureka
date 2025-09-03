use crate::error::ErrorCode;
use crate::state::HeaderChunk;
use crate::types::{UpdateClientMsg, UpdateResult};
use crate::AssembleAndUpdateClient;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak;

pub fn assemble_and_update_client(ctx: Context<AssembleAndUpdateClient>) -> Result<UpdateResult> {
    let metadata = &ctx.accounts.metadata;
    let chain_id = &metadata.chain_id;
    let target_height = metadata.target_height;

    // Collect all chunk data from remaining accounts
    let mut header_bytes = Vec::new();
    let chunk_accounts = ctx.remaining_accounts;

    require_eq!(
        chunk_accounts.len(),
        metadata.total_chunks as usize,
        ErrorCode::InvalidChunkCount
    );

    for (index, chunk_account) in chunk_accounts.iter().enumerate() {
        // Validate chunk PDA
        let expected_seeds = &[
            b"header_chunk".as_ref(),
            chain_id.as_bytes(),
            &target_height.to_le_bytes(),
            &[index as u8],
        ];
        let (expected_chunk_pda, _) = Pubkey::find_program_address(expected_seeds, ctx.program_id);
        require_eq!(
            chunk_account.key(),
            expected_chunk_pda,
            ErrorCode::InvalidChunkAccount
        );

        // Load and append chunk data
        let chunk_data = chunk_account.try_borrow_data()?;
        let chunk: HeaderChunk = HeaderChunk::try_deserialize(&mut &chunk_data[..])?;
        require_eq!(&chunk.chain_id, chain_id);
        require_eq!(chunk.target_height, target_height);
        require_eq!(chunk.chunk_index, index as u8);

        header_bytes.extend_from_slice(&chunk.chunk_data);
    }

    // Verify header commitment
    let computed_commitment = keccak::hash(&header_bytes).0;
    require!(
        metadata.header_commitment == computed_commitment,
        ErrorCode::InvalidHeader
    );

    // Now we have the full header, create UpdateClientMsg and process it
    let msg = UpdateClientMsg {
        client_message: header_bytes,
    };

    // TODO:copy logic and remove after tests succeeed
    let result = {
        let update_ctx = unsafe {
            use crate::UpdateClient;
            use core::mem;
            Context {
                program_id: ctx.program_id,
                accounts: &mut UpdateClient {
                    client_state: ctx.accounts.client_state.clone(),
                    trusted_consensus_state: ctx.accounts.trusted_consensus_state.clone(),
                    new_consensus_state_store: ctx.accounts.new_consensus_state_store.clone(),
                    payer: ctx.accounts.payer.clone(),
                    system_program: ctx.accounts.system_program.clone(),
                },
                remaining_accounts: &[],
                bumps: mem::zeroed(),
            }
        };

        crate::instructions::update_client::update_client(update_ctx, msg)?
    };

    // Close all chunk accounts to reclaim rent
    for chunk_account in chunk_accounts.iter() {
        let mut lamports = chunk_account.try_borrow_mut_lamports()?;
        let mut payer_lamports = ctx.accounts.payer.try_borrow_mut_lamports()?;
        **payer_lamports += **lamports;
        **lamports = 0;
    }

    // Metadata account will be closed automatically by Anchor due to close = payer

    msg!(
        "Successfully assembled {} chunks and updated client to height {}",
        metadata.total_chunks,
        target_height
    );
    Ok(result)
}
