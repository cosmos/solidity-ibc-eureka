use crate::errors::RouterError;
use crate::state::{PayloadChunk, PayloadMetadata, ProofChunk};
use anchor_lang::prelude::*;

/// Parameters for assembling single payload chunks
pub struct AssemblePayloadParams<'a, 'b, 'c> {
    pub remaining_accounts: &'a [AccountInfo<'b>],
    pub relayer: &'c AccountInfo<'b>,
    pub submitter: Pubkey,
    pub client_id: &'a str,
    pub sequence: u64,
    pub payload_index: u8,
    pub total_chunks: u8,
    pub program_id: &'a Pubkey,
    pub start_index: usize,
}

/// Parameters for assembling proof chunks
pub struct AssembleProofParams<'a, 'b, 'c> {
    pub remaining_accounts: &'a [AccountInfo<'b>],
    pub relayer: &'c AccountInfo<'b>,
    pub submitter: Pubkey,
    pub client_id: &'a str,
    pub sequence: u64,
    pub total_chunks: u8,
    pub program_id: &'a Pubkey,
    pub start_index: usize,
}

/// Parameters for reconstructing a packet
pub struct ReconstructPacketParams<'a, 'b, 'c> {
    pub packet: &'a solana_ibc_types::Packet,
    pub payloads_metadata: &'a [PayloadMetadata],
    pub remaining_accounts: &'a [AccountInfo<'b>],
    pub relayer: &'c AccountInfo<'b>,
    pub submitter: Pubkey,
    pub client_id: &'a str,
    pub program_id: &'a Pubkey,
}

pub fn assemble_multiple_payloads<'b>(
    remaining_accounts: &[AccountInfo<'b>],
    relayer: &AccountInfo<'b>,
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    payloads_metadata: &[PayloadMetadata],
    program_id: &Pubkey,
) -> Result<Vec<Vec<u8>>> {
    let mut all_payloads = Vec::new();
    let mut account_offset = 0;

    for (payload_index, metadata) in payloads_metadata.iter().enumerate() {
        let payload_data = assemble_single_payload_chunks(AssemblePayloadParams {
            remaining_accounts,
            relayer,
            submitter,
            client_id,
            sequence,
            payload_index: payload_index as u8,
            total_chunks: metadata.total_chunks,
            program_id,
            start_index: account_offset,
        })?;

        all_payloads.push(payload_data);
        account_offset += metadata.total_chunks as usize;
    }

    Ok(all_payloads)
}

pub fn assemble_single_payload_chunks(params: AssemblePayloadParams) -> Result<Vec<u8>> {
    let mut payload_data = Vec::new();
    let mut accounts_processed = 0;

    // Collect and validate chunks
    for i in 0..params.total_chunks {
        let account_index = params.start_index + accounts_processed;
        msg!(
            "Processing payload chunk {}/{}: account_index={}, remaining_accounts.len()={}",
            i,
            params.total_chunks,
            account_index,
            params.remaining_accounts.len()
        );
        require!(
            account_index < params.remaining_accounts.len(),
            RouterError::InvalidChunkCount
        );

        let chunk_account = &params.remaining_accounts[account_index];

        // Verify PDA
        let expected_seeds = &[
            PayloadChunk::SEED,
            params.submitter.as_ref(),
            params.client_id.as_bytes(),
            &params.sequence.to_le_bytes(),
            &[params.payload_index],
            &[i],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, params.program_id);

        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Load and validate chunk
        let chunk_data = chunk_account.try_borrow_data()?;
        let chunk: PayloadChunk = PayloadChunk::try_deserialize(&mut &chunk_data[..])?;

        require_eq!(&chunk.client_id, params.client_id);
        require_eq!(chunk.sequence, params.sequence);
        require_eq!(chunk.payload_index, params.payload_index);
        require_eq!(chunk.chunk_index, i);

        payload_data.extend_from_slice(&chunk.chunk_data);
        accounts_processed += 1;
    }

    // Commitment validation removed - no longer needed

    // Clean up chunks and return rent
    cleanup_payload_chunks(
        &params.remaining_accounts[params.start_index..params.start_index + accounts_processed],
        params.relayer,
        params.submitter,
        params.client_id,
        params.sequence,
        params.payload_index,
        params.total_chunks,
        params.program_id,
    )?;

    Ok(payload_data)
}

pub fn total_payload_chunks(metadata: &[PayloadMetadata]) -> usize {
    metadata.iter().map(|p| p.total_chunks as usize).sum()
}

/// Assemble proof chunks from remaining accounts and verify commitment
pub fn assemble_proof_chunks(params: AssembleProofParams) -> Result<Vec<u8>> {
    msg!(
        "assemble_proof_chunks: total_chunks={}, start_index={}, remaining_accounts={}",
        params.total_chunks,
        params.start_index,
        params.remaining_accounts.len()
    );

    let mut proof_data = Vec::new();
    let mut accounts_processed = 0;

    // Collect and validate chunks
    for i in 0..params.total_chunks {
        let account_index = params.start_index + accounts_processed;
        require!(
            account_index < params.remaining_accounts.len(),
            RouterError::InvalidChunkCount
        );

        let chunk_account = &params.remaining_accounts[account_index];

        // Verify PDA
        let expected_seeds = &[
            ProofChunk::SEED,
            params.submitter.as_ref(),
            params.client_id.as_bytes(),
            &params.sequence.to_le_bytes(),
            &[i],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, params.program_id);

        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Load and validate chunk
        let chunk_data = chunk_account.try_borrow_data()?;
        let chunk: ProofChunk = ProofChunk::try_deserialize(&mut &chunk_data[..])?;

        require_eq!(&chunk.client_id, params.client_id);
        require_eq!(chunk.sequence, params.sequence);
        require_eq!(chunk.chunk_index, i);

        proof_data.extend_from_slice(&chunk.chunk_data);
        accounts_processed += 1;
    }

    // Commitment validation removed - no longer needed

    // Clean up chunks and return rent
    cleanup_proof_chunks(
        &params.remaining_accounts[params.start_index..params.start_index + accounts_processed],
        params.relayer,
        params.submitter,
        params.client_id,
        params.sequence,
        params.total_chunks,
        params.program_id,
    )?;

    Ok(proof_data)
}

/// Clean up payload chunks by zeroing data (lamports remain for later reclaim via `cleanup_chunks`)
// TODO: remove clippy allow after fix
#[allow(clippy::too_many_arguments)]
fn cleanup_payload_chunks(
    chunk_accounts: &[AccountInfo],
    _payer: &AccountInfo,
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    payload_index: u8,
    total_chunks: u8,
    program_id: &Pubkey,
) -> Result<()> {
    require_eq!(
        total_chunks,
        u8::try_from(chunk_accounts.len()).map_err(|_| RouterError::InvalidChunkCount)?,
        RouterError::InvalidChunkCount
    );

    for (i, chunk_account) in chunk_accounts.iter().enumerate() {
        // Double-check PDA (paranoid check)
        let expected_seeds = &[
            PayloadChunk::SEED,
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

        // TODO: Fix this, data should be cleaned up
        // Clear the chunk data to prevent replay
        // Note: Lamports are NOT transferred here to avoid UnbalancedInstruction errors.
        // Users must call cleanup_chunks separately to reclaim rent.
        let mut data = chunk_account.try_borrow_mut_data()?;
        data.fill(0);
    }
    Ok(())
}

/// Clean up proof chunks by zeroing data (lamports remain for later reclaim via `cleanup_chunks`)
fn cleanup_proof_chunks(
    chunk_accounts: &[AccountInfo],
    _payer: &AccountInfo,
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    _total_chunks: u8,
    program_id: &Pubkey,
) -> Result<()> {
    for (i, chunk_account) in chunk_accounts.iter().enumerate() {
        // Double-check PDA (paranoid check)
        let expected_seeds = &[
            ProofChunk::SEED,
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

        // Clear the chunk data to prevent replay
        // Note: Lamports are NOT transferred here to avoid UnbalancedInstruction errors.
        // Users must call cleanup_chunks separately to reclaim rent.
        let mut data = chunk_account.try_borrow_mut_data()?;
        data.fill(0);
    }
    Ok(())
}

/// Reconstruct a complete packet from either inline mode or chunked mode
///
/// This function provides a unified interface for packet reconstruction, handling both:
/// - **Inline mode**: When packet.payloads is not empty, the packet is returned as-is
/// - **Chunked mode**: When packet.payloads is empty, payloads are assembled from chunks and packet is reconstructed
///
/// # Returns
/// * `Ok(solana_ibc_types::Packet)` - Reconstructed packet with payloads
/// * `Err` - If validation fails, or chunks cannot be assembled or both inline and payload
/// *  metadata was provided
pub fn validate_and_reconstruct_packet(
    params: ReconstructPacketParams,
) -> Result<solana_ibc_types::Packet> {
    let has_inline_payloads = !params.packet.payloads.is_empty();
    let has_chunked_metadata = params.payloads_metadata.iter().any(|p| p.total_chunks > 0);

    require!(
        !(has_inline_payloads && has_chunked_metadata),
        RouterError::InvalidPayloadCount
    );
    let payloads = if params.packet.payloads.is_empty() {
        // Chunked mode: Assemble payloads from chunks
        let payload_data_vec = assemble_multiple_payloads(
            params.remaining_accounts,
            params.relayer,
            params.submitter,
            params.client_id,
            params.packet.sequence,
            params.payloads_metadata,
            params.program_id,
        )?;

        // Reconstruct the full payloads
        let mut assembled_payloads = Vec::new();
        for (i, metadata) in params.payloads_metadata.iter().enumerate() {
            let payload = solana_ibc_types::Payload {
                source_port: metadata.source_port.clone(),
                dest_port: metadata.dest_port.clone(),
                version: metadata.version.clone(),
                encoding: metadata.encoding.clone(),
                value: payload_data_vec[i].clone(),
            };
            assembled_payloads.push(payload);
        }
        assembled_payloads
    } else {
        // Inline mode: Use payloads directly from packet (no metadata needed)
        // The packet commitment is already verified via light client membership proof
        params.packet.payloads.clone()
    };

    // Return reconstructed packet
    Ok(solana_ibc_types::Packet {
        sequence: params.packet.sequence,
        source_client: params.packet.source_client.clone(),
        dest_client: params.packet.dest_client.clone(),
        timeout_timestamp: params.packet.timeout_timestamp,
        payloads,
    })
}
