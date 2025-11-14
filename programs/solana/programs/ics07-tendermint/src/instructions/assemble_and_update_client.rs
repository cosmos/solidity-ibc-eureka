use crate::error::ErrorCode;
use crate::helpers::deserialize_header;
use crate::state::{ConsensusStateStore, HeaderChunk, CHUNK_DATA_SIZE};
use crate::types::{ConsensusState, UpdateResult};
use crate::AssembleAndUpdateClient;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::log::sol_log_compute_units;
use anchor_lang::solana_program::program::set_return_data;
use anchor_lang::system_program;
use ibc_client_tendermint::types::{ConsensusState as IbcConsensusState, Header};
use tendermint_light_client_update_client::ClientState as UpdateClientState;

pub fn assemble_and_update_client(
    mut ctx: Context<AssembleAndUpdateClient>,
    chain_id: String,
    target_height: u64,
) -> Result<UpdateResult> {
    require!(
        !ctx.accounts.client_state.is_frozen(),
        ErrorCode::ClientFrozen
    );

    let submitter = ctx.accounts.submitter.key();

    let header_bytes = assemble_chunks(&ctx, &chain_id, target_height)?;

    let result = process_header_update(&mut ctx, header_bytes)?;

    cleanup_chunks(&ctx, &chain_id, target_height, submitter)?;

    // Return the UpdateResult as bytes for callers to verify
    set_return_data(&result.try_to_vec()?);

    msg!("Update complete: {:?}", result);
    sol_log_compute_units();
    Ok(result)
}

fn assemble_chunks(
    ctx: &Context<AssembleAndUpdateClient>,
    chain_id: &str,
    target_height: u64,
) -> Result<Vec<u8>> {
    let submitter = ctx.accounts.submitter.key();
    let header_size = ctx.remaining_accounts.len() * CHUNK_DATA_SIZE;
    let mut header_bytes = Vec::with_capacity(header_size);

    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        let expected_seeds = &[
            crate::state::HeaderChunk::SEED,
            submitter.as_ref(),
            chain_id.as_bytes(),
            &target_height.to_le_bytes(),
            &[index as u8],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, &crate::ID);
        require_keys_eq!(
            chunk_account.key(),
            expected_pda,
            ErrorCode::InvalidChunkAccount
        );

        require_keys_eq!(
            *chunk_account.owner,
            crate::ID,
            ErrorCode::InvalidAccountOwner
        );

        let chunk_data = chunk_account.try_borrow_data()?;

        let chunk: HeaderChunk = HeaderChunk::try_deserialize(&mut &chunk_data[..])?;

        header_bytes.extend_from_slice(&chunk.chunk_data);
    }

    Ok(header_bytes)
}

fn process_header_update(
    ctx: &mut Context<AssembleAndUpdateClient>,
    header_bytes: Vec<u8>,
) -> Result<UpdateResult> {
    let client_state = &mut ctx.accounts.client_state;

    let header = deserialize_header(&header_bytes)?;

    let trusted_height = header.trusted_height.revision_height();

    let trusted_consensus_state = load_consensus_state(
        &ctx.accounts.trusted_consensus_state,
        client_state.key(),
        trusted_height,
    )?;

    let (new_height, new_consensus_state) = verify_and_update_header(
        client_state,
        &trusted_consensus_state.consensus_state,
        header,
    )?;

    let result = store_consensus_state(StoreConsensusStateParams {
        account: &ctx.accounts.new_consensus_state_store,
        submitter: &ctx.accounts.submitter,
        system_program: &ctx.accounts.system_program,
        client_key: client_state.key(),
        height: new_height.revision_height(),
        new_consensus_state: &new_consensus_state,
        trusted_consensus_state: &trusted_consensus_state.consensus_state,
        client_state,
    })?;

    // Update latest height only on successful update
    if result == UpdateResult::UpdateSuccess {
        client_state.latest_height = new_height.into();
    }

    Ok(result)
}

fn verify_and_update_header(
    client_state: &crate::types::ClientState,
    trusted_state: &ConsensusState,
    header: Header,
) -> Result<(ibc_core_client_types::Height, ConsensusState)> {
    let update_client_state: UpdateClientState = client_state.into();
    let trusted_ibc_state: IbcConsensusState = trusted_state.into();

    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    // Signature verification happens here using brine-ed25519 (~30k CU per signature).
    // Note: This happens AFTER header assembly. The signatures are embedded inside the header
    // data that was uploaded via chunks. We cannot use Ed25519Program because:
    // 1. Signatures come from external blockchain data (Tendermint validators)
    // 2. They're only accessible after header assembly and deserialization
    // 3. Ed25519Program requires signatures as separate instructions in the SAME transaction
    // 4. Using Ed25519Program would require double multi-tx coordination (chunks + signatures),
    //    adding 4-8 seconds of latency per update (10-20 sequential signature verifications)
    // See README "Design Decisions" section for full explanation.
    let output = tendermint_light_client_update_client::update_client(
        &update_client_state,
        &trusted_ibc_state,
        header,
        current_time,
    )
    .map_err(|e| {
        msg!("update_client FAILED: {:?}", e);
        sol_log_compute_units();
        ErrorCode::UpdateClientFailed
    })?;

    let new_consensus_state = output
        .new_consensus_state
        .try_into()
        .map_err(|_| ErrorCode::SerializationError)?;

    Ok((output.latest_height, new_consensus_state))
}

fn cleanup_chunks(
    ctx: &Context<AssembleAndUpdateClient>,
    chain_id: &str,
    target_height: u64,
    submitter: Pubkey,
) -> Result<()> {
    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        // Double-check PDA (paranoid check)
        let expected_seeds = &[
            crate::state::HeaderChunk::SEED,
            submitter.as_ref(),
            chain_id.as_bytes(),
            &target_height.to_le_bytes(),
            &[index as u8],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, &crate::ID);
        require_keys_eq!(
            chunk_account.key(),
            expected_pda,
            ErrorCode::InvalidChunkAccount
        );

        require_keys_eq!(
            *chunk_account.owner,
            crate::ID,
            ErrorCode::InvalidAccountOwner
        );

        let mut data = chunk_account.try_borrow_mut_data()?;
        data.fill(0);

        let mut lamports = chunk_account.try_borrow_mut_lamports()?;
        let mut submitter_lamports = ctx.accounts.submitter.try_borrow_mut_lamports()?;

        **submitter_lamports = submitter_lamports
            .checked_add(**lamports)
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        **lamports = 0;
    }
    Ok(())
}

// Helper function to load and validate consensus state
fn load_consensus_state(
    account: &UncheckedAccount,
    client_key: Pubkey,
    height: u64,
) -> Result<ConsensusStateStore> {
    // Validate PDA
    let (expected_pda, _) = Pubkey::find_program_address(
        &[
            crate::state::ConsensusStateStore::SEED,
            client_key.as_ref(),
            &height.to_le_bytes(),
        ],
        &crate::ID,
    );

    require_keys_eq!(
        account.key(),
        expected_pda,
        ErrorCode::AccountValidationFailed
    );

    let account_data = account.try_borrow_data()?;
    require!(!account_data.is_empty(), ErrorCode::ConsensusStateNotFound);

    ConsensusStateStore::try_deserialize(&mut &account_data[..])
        .map_err(|_| error!(ErrorCode::SerializationError))
}

struct StoreConsensusStateParams<'a, 'info> {
    account: &'a UncheckedAccount<'info>,
    submitter: &'a Signer<'info>,
    system_program: &'a Program<'info, System>,
    client_key: Pubkey,
    height: u64,
    new_consensus_state: &'a ConsensusState,
    trusted_consensus_state: &'a ConsensusState,
    client_state: &'a mut Account<'info, crate::types::ClientState>,
}

fn store_consensus_state(params: StoreConsensusStateParams) -> Result<UpdateResult> {
    // Validate PDA
    let (expected_pda, bump) = Pubkey::find_program_address(
        &[
            crate::state::ConsensusStateStore::SEED,
            params.client_key.as_ref(),
            &params.height.to_le_bytes(),
        ],
        &crate::ID,
    );

    require_keys_eq!(
        expected_pda,
        params.account.key(),
        ErrorCode::AccountValidationFailed
    );

    // Check if account exists
    if params.account.lamports() > 0 {
        // Account exists - check for conflicts
        let account_data = params.account.try_borrow_data()?;
        if !account_data.is_empty() {
            let existing: ConsensusStateStore =
                ConsensusStateStore::try_deserialize(&mut &account_data[..])?;

            let state_mismatch = existing.consensus_state != *params.new_consensus_state;
            let timestamp_not_increasing =
                params.trusted_consensus_state.timestamp >= params.new_consensus_state.timestamp;

            if state_mismatch || timestamp_not_increasing {
                params.client_state.freeze();
                return Ok(UpdateResult::Misbehaviour);
            }

            return Ok(UpdateResult::NoOp);
        }
    }

    // Create new account
    let space = 8 + ConsensusStateStore::INIT_SPACE;
    let rent = Rent::get()?.minimum_balance(space);

    let seeds_with_bump = [
        crate::state::ConsensusStateStore::SEED,
        params.client_key.as_ref(),
        &params.height.to_le_bytes(),
        &[bump],
    ];

    // IMPORTANT TODO: check again if anchor could simplify pda validation
    let cpi_accounts = system_program::CreateAccount {
        from: params.submitter.to_account_info(),
        to: params.account.to_account_info(),
    };
    let cpi_program = params.system_program.to_account_info();
    let signer = &[&seeds_with_bump[..]];
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

    system_program::create_account(cpi_ctx, rent, space as u64, &crate::ID)?;

    // Serialize the new consensus state
    let new_store = ConsensusStateStore {
        height: params.height,
        consensus_state: params.new_consensus_state.clone(),
    };

    let mut data = params.account.try_borrow_mut_data()?;
    let mut cursor = std::io::Cursor::new(&mut data[..]);
    new_store.try_serialize(&mut cursor)?;

    Ok(UpdateResult::UpdateSuccess)
}

#[cfg(test)]
mod tests;
