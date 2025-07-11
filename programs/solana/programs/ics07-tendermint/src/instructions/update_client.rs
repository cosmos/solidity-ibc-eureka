use anchor_lang::prelude::*;
use anchor_lang::system_program;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use tendermint_light_client_update_client::ClientState as UpdateClientState;
use std::io::Write;
use crate::error::ErrorCode;
use crate::helpers::deserialize_header;
use crate::state::ConsensusStateStore;
use crate::types::{ConsensusState, UpdateClientMsg};
use crate::UpdateClient;

pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<()> {
    let client_data = &mut ctx.accounts.client_data;

    require!(!client_data.frozen, ErrorCode::ClientFrozen);

    let header = deserialize_header(&msg.client_message)?;

    let client_state: UpdateClientState = client_data.client_state.clone().into();
    let trusted_consensus_state: IbcConsensusState = client_data.consensus_state.clone().into();

    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    let output = tendermint_light_client_update_client::update_client(
        &client_state,
        &trusted_consensus_state,
        header,
        current_time,
    )
    .map_err(|e| {
        msg!("Update client failed: {:?}", e);
        error!(ErrorCode::UpdateClientFailed)
    })?;

    let new_height = output.latest_height;
    let new_consensus_state: ConsensusState = output.new_consensus_state.clone().into();

    // Check for non-increasing timestamps (misbehaviour)
    if new_consensus_state.timestamp <= trusted_consensus_state.timestamp.unix_timestamp_nanos() as u64 {
        // Non-increasing timestamp - freeze the client
        client_data.frozen = true;
        msg!("Misbehaviour detected: non-increasing timestamp");
        return err!(ErrorCode::MisbehaviourNonIncreasingTime);
    }

    // Verify the consensus state PDA matches what we expect
    let (expected_pda, _bump) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_data.key().as_ref(),
            &new_height.revision_height().to_le_bytes(),
        ],
        ctx.program_id,
    );

    require!(
        expected_pda == ctx.accounts.new_consensus_state_store.key(),
        ErrorCode::AccountValidationFailed
    );

    let new_consensus_state_store = &ctx.accounts.new_consensus_state_store;

    // Check if consensus state already exists (misbehaviour)
    if !new_consensus_state_store.data_is_empty() {
        // Account exists - deserialize and check for conflicts
        let data = new_consensus_state_store.try_borrow_data()?;

        // Skip discriminator (8 bytes) and deserialize
        let existing_store: ConsensusStateStore = ConsensusStateStore::try_deserialize(&mut &data[8..])?;

        // Check for conflicting consensus state
        if existing_store.consensus_state.timestamp != new_consensus_state.timestamp
            || existing_store.consensus_state.root != new_consensus_state.root
            || existing_store.consensus_state.next_validators_hash != new_consensus_state.next_validators_hash
        {
            // Conflicting consensus state - freeze the client
            client_data.frozen = true;
            msg!("Misbehaviour detected: conflicting consensus state at height {}", new_height.revision_height());
            return err!(ErrorCode::MisbehaviourConflictingState);
        }

        // Same consensus state already exists - this is a no-op
        msg!("Consensus state already exists at height {}", new_height.revision_height());
    } else {
        // Create new consensus state account
        let space = 8 + 8 + 8 + 32 + 32; // discriminator + height + timestamp + root + next_validators_hash
        let rent = Rent::get()?.minimum_balance(space);

        // Create the account
        system_program::create_account(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::CreateAccount {
                    from: ctx.accounts.payer.to_account_info(),
                    to: new_consensus_state_store.to_account_info(),
                },
            ),
            rent,
            space as u64,
            ctx.program_id,
        )?;

        let mut data = new_consensus_state_store.try_borrow_mut_data()?;
        let mut cursor = std::io::Cursor::new(&mut data[..]);

        // NOTE: Anchor requires all accounts to start with an 8-byte discriminator that identifies
        // the account type. This is the SHA256 hash of "account:ConsensusStateStore" (first 8 bytes).
        // We write it manually here because we're creating the account using system_program::create_account
        // instead of Anchor's init constraint, which would normally handle this automatically.
        // We do manual creation to check for existing accounts first (for misbehaviour detection).
        let discriminator = [
            217, 208, 130, 233, 170, 148, 153, 101
        ];
        cursor.write_all(&discriminator)?;

        // Write the consensus state data
        let store = ConsensusStateStore {
            height: new_height.revision_height(),
            consensus_state: new_consensus_state.clone(),
        };
        store.try_serialize(&mut cursor)?;
    }

    // Update client state and consensus state
    client_data.client_state.latest_height = new_height.into();
    client_data.consensus_state = new_consensus_state;

    Ok(())
}
