use crate::error::ErrorCode;
use crate::helpers::deserialize_header;
use crate::state::{ConsensusStateStore, HeaderChunk};
use crate::types::{ConsensusState, UpdateResult};
use crate::AssembleAndUpdateClient;
use anchor_lang::prelude::*;
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

    Ok(result)
}

fn assemble_chunks(
    ctx: &Context<AssembleAndUpdateClient>,
    chain_id: &str,
    target_height: u64,
) -> Result<Vec<u8>> {
    let submitter = ctx.accounts.submitter.key();
    let mut header_bytes = Vec::new();

    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        validate_and_load_chunk(
            chunk_account,
            chain_id,
            target_height,
            submitter,
            index as u8,
            ctx.program_id,
            &mut header_bytes,
        )?;
    }

    Ok(header_bytes)
}

fn validate_and_load_chunk(
    chunk_account: &AccountInfo,
    chain_id: &str,
    target_height: u64,
    submitter: Pubkey,
    index: u8,
    program_id: &Pubkey,
    header_bytes: &mut Vec<u8>,
) -> Result<()> {
    // Validate chunk PDA
    let expected_seeds = &[
        crate::state::HeaderChunk::SEED,
        submitter.as_ref(),
        chain_id.as_bytes(),
        &target_height.to_le_bytes(),
        &[index],
    ];
    let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, program_id);
    require_eq!(
        chunk_account.key(),
        expected_pda,
        ErrorCode::InvalidChunkAccount
    );

    let chunk_data = chunk_account.try_borrow_data()?;
    let chunk: HeaderChunk = HeaderChunk::try_deserialize(&mut &chunk_data[..])?;

    header_bytes.extend_from_slice(&chunk.chunk_data);
    Ok(())
}

fn process_header_update(
    ctx: &mut Context<AssembleAndUpdateClient>,
    header_bytes: Vec<u8>,
) -> Result<UpdateResult> {
    let client_state = &mut ctx.accounts.client_state;

    // Deserialize and validate header
    let header = deserialize_header(&header_bytes)?;
    let trusted_height = header.trusted_height.revision_height();

    let trusted_consensus_state = load_consensus_state(
        &ctx.accounts.trusted_consensus_state,
        client_state.key(),
        trusted_height,
        ctx.program_id,
    )?;

    let (new_height, new_consensus_state) = verify_and_update_header(
        client_state,
        &trusted_consensus_state.consensus_state,
        header,
    )?;

    check_misbehaviour(
        &new_consensus_state,
        &trusted_consensus_state.consensus_state,
        client_state,
    )?;

    let result = store_consensus_state(StoreConsensusStateParams {
        account: &ctx.accounts.new_consensus_state_store,
        payer: &ctx.accounts.payer,
        system_program: &ctx.accounts.system_program,
        program_id: ctx.program_id,
        client_key: client_state.key(),
        height: new_height.revision_height(),
        new_consensus_state: &new_consensus_state,
        client_state,
    })?;

    if result == UpdateResult::Update {
        client_state.latest_height = new_height.into();
    }

    Ok(result)
}

fn verify_and_update_header(
    client_state: &crate::types::ClientState,
    trusted_state: &ConsensusState,
    header: Header,
) -> Result<(ibc_core_client_types::Height, ConsensusState)> {
    let update_client_state: UpdateClientState = client_state.clone().into();
    let trusted_ibc_state: IbcConsensusState = trusted_state.clone().into();
    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    let output = tendermint_light_client_update_client::update_client(
        &update_client_state,
        &trusted_ibc_state,
        header,
        current_time,
    )
    .map_err(|e| {
        msg!("Header verification failed: {:?}", e);
        ErrorCode::UpdateClientFailed
    })?;

    Ok((
        output.latest_height,
        output
            .new_consensus_state
            .try_into()
            .map_err(|_| ErrorCode::SerializationError)?,
    ))
}

fn check_misbehaviour(
    new_state: &ConsensusState,
    trusted_state: &ConsensusState,
    client_state: &mut Account<crate::types::ClientState>,
) -> Result<()> {
    let trusted_ibc: IbcConsensusState = trusted_state.clone().into();
    let trusted_timestamp = trusted_ibc.timestamp.unix_timestamp_nanos() as u64;

    if new_state.timestamp <= trusted_timestamp {
        client_state.freeze();
        return err!(ErrorCode::MisbehaviourNonIncreasingTime);
    }
    Ok(())
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
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, ctx.program_id);
        require_eq!(
            chunk_account.key(),
            expected_pda,
            ErrorCode::InvalidChunkAccount
        );

        let mut lamports = chunk_account.try_borrow_mut_lamports()?;
        let mut submitter_lamports = ctx.accounts.submitter.try_borrow_mut_lamports()?;
        **submitter_lamports += **lamports;
        **lamports = 0;
    }
    Ok(())
}

// Helper function to load and validate consensus state
fn load_consensus_state(
    account: &UncheckedAccount,
    client_key: Pubkey,
    height: u64,
    program_id: &Pubkey,
) -> Result<ConsensusStateStore> {
    // Validate PDA
    let (expected_pda, _) = Pubkey::find_program_address(
        &[
            crate::state::ConsensusStateStore::SEED,
            client_key.as_ref(),
            &height.to_le_bytes(),
        ],
        program_id,
    );

    require!(
        expected_pda == account.key(),
        ErrorCode::AccountValidationFailed
    );

    let account_data = account.try_borrow_data()?;
    require!(!account_data.is_empty(), ErrorCode::ConsensusStateNotFound);

    ConsensusStateStore::try_deserialize(&mut &account_data[..])
        .map_err(|_| error!(ErrorCode::SerializationError))
}

struct StoreConsensusStateParams<'a, 'info> {
    account: &'a UncheckedAccount<'info>,
    payer: &'a Signer<'info>,
    system_program: &'a Program<'info, System>,
    program_id: &'a Pubkey,
    client_key: Pubkey,
    height: u64,
    new_consensus_state: &'a ConsensusState,
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
        params.program_id,
    );

    require!(
        expected_pda == params.account.key(),
        ErrorCode::AccountValidationFailed
    );

    // Check if account exists
    if params.account.lamports() > 0 {
        // Account exists - check for conflicts
        let account_data = params.account.try_borrow_data()?;
        if !account_data.is_empty() {
            let existing: ConsensusStateStore =
                ConsensusStateStore::try_deserialize(&mut &account_data[..])?;

            if existing.consensus_state != *params.new_consensus_state {
                params.client_state.freeze();
                return err!(ErrorCode::MisbehaviourConflictingState);
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

    let cpi_accounts = system_program::CreateAccount {
        from: params.payer.to_account_info(),
        to: params.account.to_account_info(),
    };
    let cpi_program = params.system_program.to_account_info();
    let signer = &[&seeds_with_bump[..]];
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

    system_program::create_account(cpi_ctx, rent, space as u64, params.program_id)?;

    // Serialize the new consensus state
    let new_store = ConsensusStateStore {
        height: params.height,
        consensus_state: params.new_consensus_state.clone(),
    };

    let mut data = params.account.try_borrow_mut_data()?;
    let mut cursor = std::io::Cursor::new(&mut data[..]);
    new_store.try_serialize(&mut cursor)?;

    Ok(UpdateResult::Update)
}

#[cfg(test)]
mod tests;
