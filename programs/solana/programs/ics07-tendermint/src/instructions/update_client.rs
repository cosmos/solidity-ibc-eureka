use crate::error::ErrorCode;
use crate::helpers::deserialize_header;
use crate::state::ConsensusStateStore;
use crate::types::{ClientState, ConsensusState, UpdateClientMsg, UpdateResult};
use crate::UpdateClient;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::pubkey::Pubkey;
use anchor_lang::system_program;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use ibc_core_client_types::Height;
use std::io::Write;
use tendermint_light_client_update_client::ClientState as UpdateClientState;

pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<UpdateResult> {
    let client_state = &mut ctx.accounts.client_state;

    require!(!client_state.is_frozen(), ErrorCode::ClientFrozen);

    // Extract trusted height from header
    let header = deserialize_header(&msg.client_message)?;
    let trusted_height = header.trusted_height;

    // Validate and load the trusted consensus state
    let trusted_consensus_state = validate_and_load_trusted_state(
        &ctx.accounts.trusted_consensus_state,
        client_state.key(),
        trusted_height.revision_height(),
        ctx.program_id,
    )?;

    let (new_height, new_consensus_state) = verify_header_and_get_state(
        client_state.as_ref(),
        &trusted_consensus_state.consensus_state,
        &msg.client_message,
    )?;

    check_timestamp_misbehaviour(
        &new_consensus_state,
        &trusted_consensus_state.consensus_state,
        client_state,
    )?;

    verify_consensus_state_pda(
        &ctx.accounts.new_consensus_state_store,
        client_state.key(),
        new_height.revision_height(),
        ctx.program_id,
    )?;

    let update_result = handle_consensus_state_storage(
        &ctx.accounts.new_consensus_state_store,
        &ctx.accounts.payer,
        &ctx.accounts.system_program,
        ctx.program_id,
        new_height.revision_height(),
        &new_consensus_state,
        client_state,
    )?;

    if let UpdateResult::Update = update_result {
        client_state.latest_height = new_height.into();
    }

    Ok(update_result)
}

fn validate_and_load_trusted_state<'info>(
    trusted_consensus_state_account: &UncheckedAccount<'info>,
    client_key: Pubkey,
    trusted_height: u64,
    program_id: &Pubkey,
) -> Result<ConsensusStateStore> {
    // Validate the PDA
    let (expected_pda, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_key.as_ref(),
            &trusted_height.to_le_bytes(),
        ],
        program_id,
    );

    require!(
        expected_pda == trusted_consensus_state_account.key(),
        ErrorCode::AccountValidationFailed
    );

    // Load and verify the account exists
    let account_data = trusted_consensus_state_account.try_borrow_data()?;
    require!(!account_data.is_empty(), ErrorCode::ConsensusStateNotFound);

    // Deserialize the consensus state (skip 8-byte discriminator)
    ConsensusStateStore::try_deserialize(&mut &account_data[8..])
}

fn check_timestamp_misbehaviour(
    new_consensus_state: &ConsensusState,
    trusted_consensus_state: &ConsensusState,
    client_state: &mut ClientState,
) -> Result<()> {
    let trusted_timestamp = Into::<IbcConsensusState>::into(trusted_consensus_state.clone())
        .timestamp
        .unix_timestamp_nanos() as u64;

    if new_consensus_state.timestamp <= trusted_timestamp {
        client_state.freeze();
        msg!("Misbehaviour detected: non-increasing timestamp");
        return err!(ErrorCode::MisbehaviourNonIncreasingTime);
    }

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
    client_state: &mut ClientState,
) -> Result<UpdateResult> {
    if !new_consensus_state_store.data_is_empty() {
        // Consensus state already exists at this height - check for misbehaviour
        check_existing_consensus_state(
            new_consensus_state_store,
            new_consensus_state,
            revision_height,
            client_state,
        )
    } else {
        // Create new consensus state account
        create_consensus_state_account(
            new_consensus_state_store,
            payer,
            system_program,
            program_id,
            revision_height,
            new_consensus_state,
        )?;
        Ok(UpdateResult::Update)
    }
}

fn check_existing_consensus_state(
    new_consensus_state_store: &UncheckedAccount,
    new_consensus_state: &ConsensusState,
    revision_height: u64,
    client_state: &mut ClientState,
) -> Result<UpdateResult> {
    let data = new_consensus_state_store.try_borrow_data()?;
    let existing_store: ConsensusStateStore =
        ConsensusStateStore::try_deserialize(&mut &data[8..])?;

    if existing_store.consensus_state.timestamp != new_consensus_state.timestamp
        || existing_store.consensus_state.root != new_consensus_state.root
        || existing_store.consensus_state.next_validators_hash
            != new_consensus_state.next_validators_hash
    {
        client_state.freeze();
        msg!(
            "Misbehaviour detected: conflicting consensus state at height {}",
            revision_height
        );

        return err!(ErrorCode::MisbehaviourConflictingConsensusState);
    }

    msg!(
        "Consensus state already exists at height {}",
        revision_height
    );
    Ok(UpdateResult::NoOp)
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
