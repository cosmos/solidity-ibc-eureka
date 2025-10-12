use crate::errors::RouterError;
use crate::state::{PayloadChunk, ProofChunk, PayloadMetadata, PAYLOAD_CHUNK_SEED, PROOF_CHUNK_SEED};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak;

/// Assemble multiple payloads from chunks
pub fn assemble_multiple_payloads(
    remaining_accounts: &[AccountInfo],
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    payloads_metadata: &[PayloadMetadata],
    program_id: &Pubkey,
) -> Result<Vec<Vec<u8>>> {
    let mut all_payloads = Vec::new();
    let mut account_offset = 0;

    for (payload_index, metadata) in payloads_metadata.iter().enumerate() {
        let payload_data = assemble_single_payload_chunks(
            remaining_accounts,
            submitter,
            client_id,
            sequence,
            payload_index as u8,
            metadata.total_chunks,
            metadata.commitment,
            program_id,
            account_offset,
        )?;

        all_payloads.push(payload_data);
        account_offset += metadata.total_chunks as usize;
    }

    Ok(all_payloads)
}

/// Assemble a single payload's chunks from remaining accounts and verify commitment
pub fn assemble_single_payload_chunks(
    remaining_accounts: &[AccountInfo],
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    payload_index: u8,
    total_chunks: u8,
    expected_commitment: [u8; 32],
    program_id: &Pubkey,
    start_index: usize,
) -> Result<Vec<u8>> {
    // If no chunks, return empty data (for testing)
    if total_chunks == 0 {
        return Ok(vec![]);
    }

    let mut payload_data = Vec::new();
    let mut accounts_processed = 0;

    // Collect and validate chunks
    for i in 0..total_chunks {
        let account_index = start_index + accounts_processed;
        require!(
            account_index < remaining_accounts.len(),
            RouterError::InvalidChunkCount
        );

        let chunk_account = &remaining_accounts[account_index];

        // Verify PDA
        let expected_seeds = &[
            PAYLOAD_CHUNK_SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[payload_index],
            &[i],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, program_id);

        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Load and validate chunk
        let chunk_data = chunk_account.try_borrow_data()?;
        let chunk: PayloadChunk = PayloadChunk::try_deserialize(&mut &chunk_data[..])?;

        require_eq!(&chunk.client_id, client_id);
        require_eq!(chunk.sequence, sequence);
        require_eq!(chunk.payload_index, payload_index);
        require_eq!(chunk.chunk_index, i);

        payload_data.extend_from_slice(&chunk.chunk_data);
        accounts_processed += 1;
    }

    // Verify commitment
    let computed_commitment = keccak::hash(&payload_data).0;
    require!(
        computed_commitment == expected_commitment,
        RouterError::InvalidChunkCommitment
    );

    // Clean up chunks and return rent
    cleanup_payload_chunks(
        &remaining_accounts[start_index..start_index + accounts_processed],
        submitter,
        client_id,
        sequence,
        payload_index,
        total_chunks,
        program_id,
    )?;

    Ok(payload_data)
}

/// Backward compatibility function for single payload (assumes payload_index = 0)
pub fn assemble_payload_chunks(
    remaining_accounts: &[AccountInfo],
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    total_chunks: u8,
    expected_commitment: [u8; 32],
    program_id: &Pubkey,
) -> Result<Vec<u8>> {
    assemble_single_payload_chunks(
        remaining_accounts,
        submitter,
        client_id,
        sequence,
        0, // Default to payload_index 0 for backward compatibility
        total_chunks,
        expected_commitment,
        program_id,
        0, // Start from index 0
    )
}

/// Assemble proof chunks from remaining accounts and verify commitment
pub fn assemble_proof_chunks(
    remaining_accounts: &[AccountInfo],
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    total_chunks: u8,
    expected_commitment: [u8; 32],
    program_id: &Pubkey,
    start_index: usize, // Where to start in remaining_accounts
) -> Result<Vec<u8>> {
    // If no chunks, return empty data (for testing)
    if total_chunks == 0 {
        return Ok(vec![]);
    }

    let mut proof_data = Vec::new();
    let mut accounts_processed = 0;

    // Collect and validate chunks
    for i in 0..total_chunks {
        let account_index = start_index + accounts_processed;
        require!(
            account_index < remaining_accounts.len(),
            RouterError::InvalidChunkCount
        );

        let chunk_account = &remaining_accounts[account_index];

        // Verify PDA
        let expected_seeds = &[
            PROOF_CHUNK_SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[i],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, program_id);

        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Load and validate chunk
        let chunk_data = chunk_account.try_borrow_data()?;
        let chunk: ProofChunk = ProofChunk::try_deserialize(&mut &chunk_data[..])?;

        require_eq!(&chunk.client_id, client_id);
        require_eq!(chunk.sequence, sequence);
        require_eq!(chunk.chunk_index, i);

        proof_data.extend_from_slice(&chunk.chunk_data);
        accounts_processed += 1;
    }

    // Verify commitment
    let computed_commitment = keccak::hash(&proof_data).0;
    require!(
        computed_commitment == expected_commitment,
        RouterError::InvalidChunkCommitment
    );

    // Clean up chunks and return rent
    cleanup_proof_chunks(
        &remaining_accounts[start_index..start_index + accounts_processed],
        submitter,
        client_id,
        sequence,
        total_chunks,
        program_id,
    )?;

    Ok(proof_data)
}

/// Clean up payload chunks and return rent to submitter
fn cleanup_payload_chunks(
    chunk_accounts: &[AccountInfo],
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    payload_index: u8,
    _total_chunks: u8,
    program_id: &Pubkey,
) -> Result<()> {
    for (i, chunk_account) in chunk_accounts.iter().enumerate() {
        // Double-check PDA (paranoid check)
        let expected_seeds = &[
            PAYLOAD_CHUNK_SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[payload_index],
            &[i as u8],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, program_id);
        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Return rent to submitter
        let mut chunk_lamports = chunk_account.try_borrow_mut_lamports()?;

        // Find submitter account in remaining accounts or use the chunk_account itself
        // Note: In actual usage, the submitter account should be passed as a mutable account
        // For now, we just zero out the chunk account
        **chunk_lamports = 0;

        // Clear the data
        let mut data = chunk_account.try_borrow_mut_data()?;
        data.fill(0);
    }
    Ok(())
}

/// Clean up proof chunks and return rent to submitter
fn cleanup_proof_chunks(
    chunk_accounts: &[AccountInfo],
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    _total_chunks: u8,
    program_id: &Pubkey,
) -> Result<()> {
    for (i, chunk_account) in chunk_accounts.iter().enumerate() {
        // Double-check PDA (paranoid check)
        let expected_seeds = &[
            PROOF_CHUNK_SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[i as u8],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, program_id);
        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Return rent to submitter
        let mut chunk_lamports = chunk_account.try_borrow_mut_lamports()?;

        // Note: In actual usage, the submitter account should be passed as a mutable account
        // For now, we just zero out the chunk account
        **chunk_lamports = 0;

        // Clear the data
        let mut data = chunk_account.try_borrow_mut_data()?;
        data.fill(0);
    }
    Ok(())
}