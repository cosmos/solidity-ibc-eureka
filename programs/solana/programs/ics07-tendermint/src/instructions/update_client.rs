use anchor_lang::prelude::*;
use anchor_lang::system_program;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use ibc_core_client_types::Height;
use tendermint_light_client_update_client::ClientState as UpdateClientState;
use std::io::Write;
use crate::error::ErrorCode;
use crate::helpers::deserialize_header;
use crate::state::{ClientData, ConsensusStateStore};
use crate::types::{ClientState, ConsensusState, UpdateClientMsg};
use crate::UpdateClient;

pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<()> {
    let client_data = &mut ctx.accounts.client_data;

    require!(!client_data.frozen, ErrorCode::ClientFrozen);

    let (new_height, new_consensus_state) = verify_header_and_get_state(
        &client_data.client_state,
        &client_data.consensus_state,
        &msg.client_message,
    )?;

    check_misbehaviour_timestamp(
        &new_consensus_state,
        &client_data.consensus_state.clone().into(),
        client_data,
    )?;

    verify_consensus_state_pda(
        &ctx.accounts.new_consensus_state_store,
        client_data.key(),
        new_height.revision_height(),
        ctx.program_id,
    )?;

    handle_consensus_state_storage(
        &ctx.accounts.new_consensus_state_store,
        &ctx.accounts.payer,
        &ctx.accounts.system_program,
        ctx.program_id,
        new_height.revision_height(),
        &new_consensus_state,
        client_data,
    )?;

    client_data.client_state.latest_height = new_height.into();
    client_data.consensus_state = new_consensus_state;

    Ok(())
}

fn verify_header_and_get_state(
    client_state: &ClientState,
    consensus_state: &ConsensusState,
    client_message: &[u8],
) -> Result<(Height, ConsensusState)> {
    let header = deserialize_header(client_message)?;
    let update_client_state: UpdateClientState = client_state.clone().into();
    let trusted_consensus_state: IbcConsensusState = consensus_state.clone().into();
    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    let output = tendermint_light_client_update_client::update_client(
        &update_client_state,
        &trusted_consensus_state,
        header,
        current_time,
    )
    .map_err(|e| {
        msg!("Update client failed: {:?}", e);
        error!(ErrorCode::UpdateClientFailed)
    })?;

    Ok((output.latest_height, output.new_consensus_state.into()))
}

fn check_misbehaviour_timestamp(
    new_consensus_state: &ConsensusState,
    trusted_consensus_state: &IbcConsensusState,
    client_data: &mut ClientData,
) -> Result<()> {
    if new_consensus_state.timestamp <= trusted_consensus_state.timestamp.unix_timestamp_nanos() as u64 {
        client_data.frozen = true;
        msg!("Misbehaviour detected: non-increasing timestamp");
        return err!(ErrorCode::MisbehaviourNonIncreasingTime);
    }
    Ok(())
}

fn verify_consensus_state_pda(
    new_consensus_state_store: &UncheckedAccount,
    client_key: Pubkey,
    revision_height: u64,
    program_id: &Pubkey,
) -> Result<()> {
    let (expected_pda, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_key.as_ref(),
            &revision_height.to_le_bytes(),
        ],
        program_id,
    );

    require!(
        expected_pda == new_consensus_state_store.key(),
        ErrorCode::AccountValidationFailed
    );

    Ok(())
}

fn handle_consensus_state_storage<'info>(
    new_consensus_state_store: &UncheckedAccount<'info>,
    payer: &Signer<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
    revision_height: u64,
    new_consensus_state: &ConsensusState,
    client_data: &mut ClientData,
) -> Result<()> {
    if !new_consensus_state_store.data_is_empty() {
        check_existing_consensus_state(
            new_consensus_state_store,
            new_consensus_state,
            revision_height,
            client_data,
        )?;
    } else {
        create_consensus_state_account(
            new_consensus_state_store,
            payer,
            system_program,
            program_id,
            revision_height,
            new_consensus_state,
        )?;
    }
    Ok(())
}

fn check_existing_consensus_state(
    new_consensus_state_store: &UncheckedAccount,
    new_consensus_state: &ConsensusState,
    revision_height: u64,
    client_data: &mut ClientData,
) -> Result<()> {
    let data = new_consensus_state_store.try_borrow_data()?;
    let existing_store: ConsensusStateStore = ConsensusStateStore::try_deserialize(&mut &data[8..])?;

    if existing_store.consensus_state.timestamp != new_consensus_state.timestamp
        || existing_store.consensus_state.root != new_consensus_state.root
        || existing_store.consensus_state.next_validators_hash != new_consensus_state.next_validators_hash
    {
        client_data.frozen = true;
        msg!("Misbehaviour detected: conflicting consensus state at height {}", revision_height);
        return err!(ErrorCode::MisbehaviourConflictingState);
    }

    msg!("Consensus state already exists at height {}", revision_height);
    Ok(())
}

fn create_consensus_state_account<'info>(
    new_consensus_state_store: &UncheckedAccount<'info>,
    payer: &Signer<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
    revision_height: u64,
    new_consensus_state: &ConsensusState,
) -> Result<()> {
    let space = 8 + 8 + 8 + 32 + 32;
    let rent = Rent::get()?.minimum_balance(space);

    system_program::create_account(
        CpiContext::new(
            system_program.to_account_info(),
            system_program::CreateAccount {
                from: payer.to_account_info(),
                to: new_consensus_state_store.to_account_info(),
            },
        ),
        rent,
        space as u64,
        program_id,
    )?;

    let mut data = new_consensus_state_store.try_borrow_mut_data()?;
    let mut cursor = std::io::Cursor::new(&mut data[..]);

    // NOTE: Anchor requires all accounts to start with an 8-byte discriminator that identifies
    // the account type. This is the SHA256 hash of "account:ConsensusStateStore" (first 8 bytes).
    // We write it manually here because we're creating the account using system_program::create_account
    // instead of Anchor's init constraint, which would normally handle this automatically.
    // We do manual creation to check for existing accounts first (for misbehaviour detection).
    let discriminator = [217, 208, 130, 233, 170, 148, 153, 101];
    cursor.write_all(&discriminator)?;

    let store = ConsensusStateStore {
        height: revision_height,
        consensus_state: new_consensus_state.clone(),
    };
    store.try_serialize(&mut cursor)?;

    Ok(())
}
