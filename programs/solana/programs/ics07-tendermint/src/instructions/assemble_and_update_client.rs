use crate::error::ErrorCode;
use crate::state::{HeaderChunk, HeaderMetadata};
use crate::types::{UpdateClientMsg, UpdateResult};
use crate::AssembleAndUpdateClient;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak;

pub fn assemble_and_update_client(
    ctx: Context<AssembleAndUpdateClient>,
    chain_id: String,
    target_height: u64,
) -> Result<UpdateResult> {
    let metadata = &ctx.accounts.metadata;

    // Validate metadata
    require_eq!(&metadata.chain_id, &chain_id);
    require_eq!(metadata.target_height, target_height);

    // Collect all chunk data from remaining accounts
    let mut header_bytes = Vec::new();
    let chunk_accounts = ctx.remaining_accounts;

    require_eq!(
        chunk_accounts.len(),
        metadata.total_chunks as usize,
        ErrorCode::InvalidChunkCount
    );

    for (i, chunk_account) in chunk_accounts.iter().enumerate() {
        // Validate chunk PDA
        let expected_seeds = &[
            b"header_chunk".as_ref(),
            chain_id.as_bytes(),
            &target_height.to_le_bytes(),
            &[i as u8],
        ];
        let (expected_chunk_pda, _) =
            Pubkey::find_program_address(expected_seeds, ctx.program_id);
        require_eq!(
            chunk_account.key(),
            expected_chunk_pda,
            ErrorCode::InvalidChunkAccount
        );

        // Load and append chunk data
        let chunk_data = chunk_account.try_borrow_data()?;
        let chunk: HeaderChunk = HeaderChunk::try_deserialize(&mut &chunk_data[..])?;
        require_eq!(&chunk.chain_id, &chain_id);
        require_eq!(chunk.target_height, target_height);
        require_eq!(chunk.chunk_index, i as u8);

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

    // Call the update_client handler with a synthetic context
    // This is simpler than duplicating all the logic
    let result = {
        // We need to create a synthetic UpdateClient context
        // Since we can't use BTreeMap, we'll pass an empty bumps via unsafe transmute
        // This is safe because UpdateClient doesn't use any bumps
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
                bumps: mem::zeroed(), // Safe because UpdateClient has no PDA bumps
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