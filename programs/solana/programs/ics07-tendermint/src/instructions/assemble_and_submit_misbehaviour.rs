use crate::error::ErrorCode;
use crate::helpers::deserialize_misbehaviour;
use crate::state::MisbehaviourChunk;
use crate::AssembleAndSubmitMisbehaviour;
use anchor_lang::prelude::*;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use tendermint_light_client_update_client::ClientState as TmClientState;

pub fn assemble_and_submit_misbehaviour(
    mut ctx: Context<AssembleAndSubmitMisbehaviour>,
    client_id: String,
) -> Result<()> {
    require!(
        !ctx.accounts.client_state.is_frozen(),
        ErrorCode::ClientAlreadyFrozen
    );

    let submitter = ctx.accounts.submitter.key();

    let misbehaviour_bytes = assemble_chunks(&ctx, &client_id, submitter)?;

    process_misbehaviour(&mut ctx, misbehaviour_bytes)?;

    cleanup_chunks(&ctx, &client_id, submitter)?;

    Ok(())
}

fn assemble_chunks(
    ctx: &Context<AssembleAndSubmitMisbehaviour>,
    client_id: &str,
    submitter: Pubkey,
) -> Result<Vec<u8>> {
    let mut misbehaviour_bytes = Vec::new();

    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        validate_and_load_chunk(
            chunk_account,
            client_id,
            submitter,
            index as u8,
            &mut misbehaviour_bytes,
        )?;
    }

    Ok(misbehaviour_bytes)
}

fn validate_and_load_chunk(
    chunk_account: &AccountInfo,
    client_id: &str,
    submitter: Pubkey,
    index: u8,
    misbehaviour_bytes: &mut Vec<u8>,
) -> Result<()> {
    let expected_seeds = &[
        crate::state::MisbehaviourChunk::SEED,
        submitter.as_ref(),
        client_id.as_bytes(),
        &[index],
    ];
    let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, &crate::ID);
    require_eq!(
        chunk_account.key(),
        expected_pda,
        ErrorCode::InvalidChunkAccount
    );

    let chunk_data = chunk_account.try_borrow_data()?;
    let chunk: MisbehaviourChunk = MisbehaviourChunk::try_deserialize(&mut &chunk_data[..])?;

    misbehaviour_bytes.extend_from_slice(&chunk.chunk_data);
    Ok(())
}

fn process_misbehaviour(
    ctx: &mut Context<AssembleAndSubmitMisbehaviour>,
    misbehaviour_bytes: Vec<u8>,
) -> Result<()> {
    let client_state = &ctx.accounts.client_state;

    let misbehaviour = deserialize_misbehaviour(&misbehaviour_bytes)?;
    let tm_client_state: TmClientState = client_state.clone().into_inner().into();

    let trusted_consensus_state_1: IbcConsensusState = ctx
        .accounts
        .trusted_consensus_state_1
        .consensus_state
        .clone()
        .into();
    let trusted_consensus_state_2: IbcConsensusState = ctx
        .accounts
        .trusted_consensus_state_2
        .consensus_state
        .clone()
        .into();

    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    let output = tendermint_light_client_misbehaviour::check_for_misbehaviour(
        &tm_client_state,
        &misbehaviour,
        trusted_consensus_state_1,
        trusted_consensus_state_2,
        current_time,
    )
    .map_err(|_| error!(ErrorCode::MisbehaviourCheckFailed))?;

    require!(
        ctx.accounts.trusted_consensus_state_1.height == output.trusted_height_1.revision_height(),
        ErrorCode::HeightMismatch
    );
    require!(
        ctx.accounts.trusted_consensus_state_2.height == output.trusted_height_2.revision_height(),
        ErrorCode::HeightMismatch
    );

    // If we reach here, misbehaviour was detected
    ctx.accounts.client_state.freeze();

    msg!(
        "Misbehaviour detected at heights {:?} and {:?}",
        output.trusted_height_1,
        output.trusted_height_2
    );

    Ok(())
}

fn cleanup_chunks(
    ctx: &Context<AssembleAndSubmitMisbehaviour>,
    client_id: &str,
    submitter: Pubkey,
) -> Result<()> {
    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        let expected_seeds = &[
            crate::state::MisbehaviourChunk::SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &[index as u8],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, &crate::ID);
        require_eq!(
            chunk_account.key(),
            expected_pda,
            ErrorCode::InvalidChunkAccount
        );

        let mut data = chunk_account.try_borrow_mut_data()?;
        data.fill(0);

        let mut lamports = chunk_account.try_borrow_mut_lamports()?;
        let mut submitter_lamports = ctx.accounts.submitter.try_borrow_mut_lamports()?;

        **submitter_lamports += **lamports;
        **lamports = 0;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
