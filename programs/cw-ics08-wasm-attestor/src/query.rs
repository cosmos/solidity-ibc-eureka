//! This module contains the query message handlers

use attestor_light_client::header::Header;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env};

use crate::{
    msg::{
        CheckForMisbehaviourMsg, Status, StatusResult, TimestampAtHeightMsg,
        TimestampAtHeightResult, VerifyClientMessageMsg,
    },
    state::{
        get_client_state, get_consensus_state, get_next_consensus_state,
        get_previous_consensus_state,
    },
    ContractError,
};

/// Verifies the client message (header) that will be used for updating the state of the light client
/// The actual verification logic is done in the solana light client package
/// # Errors
/// Returns an error if the client message is invalid
/// # Returns
/// An empty response
#[allow(clippy::needless_pass_by_value)]
pub fn verify_client_message(
    deps: Deps,
    verify_client_message_msg: VerifyClientMessageMsg,
) -> Result<Binary, ContractError> {
    let client_state = get_client_state(deps.storage)?;

    if let Ok(header) = serde_json::from_slice::<Header>(&verify_client_message_msg.client_message)
    {
        let consensus_maybe_exists = get_consensus_state(deps.storage, header.new_height);
        let (current, prev, next) = match consensus_maybe_exists {
            Ok(exists) => (Some(exists), None, None),
            Err(e) => match e {
                ContractError::ConsensusStateNotFound => (
                    None,
                    get_previous_consensus_state(deps.storage, header.new_height)?,
                    get_next_consensus_state(deps.storage, header.new_height)?,
                ),
                _ => return Err(e),
            },
        };
        attestor_light_client::verify::verify_header(
            current.as_ref(),
            prev.as_ref(),
            next.as_ref(),
            &client_state,
            &header,
        )
        .map_err(ContractError::VerifyClientMessageFailed)?;

        return Ok(Binary::default());
    }

    Err(ContractError::InvalidClientMessage)
}

/// Checks for misbehaviour. Returning an error means no misbehaviour was found.
///
/// Note that we are replicating some of the logic of `verify_client_message` here, ideally we
/// would also check for misbehaviour of the header in this function.
/// # Errors
/// Returns an error if the misbehaviour cannot be verified
#[allow(clippy::needless_pass_by_value)]
pub fn check_for_misbehaviour(
    _deps: Deps,
    _env: Env,
    _check_for_misbehaviour_msg: CheckForMisbehaviourMsg,
) -> Result<Binary, ContractError> {
    todo!()
}

/// Gets the consensus timestamp at a given height
/// # Errors
/// Returns an error if the consensus state is not found
/// # Returns
/// The timestamp at the given height
#[allow(clippy::needless_pass_by_value)]
pub fn timestamp_at_height(
    deps: Deps,
    timestamp_at_height_msg: TimestampAtHeightMsg,
) -> Result<Binary, ContractError> {
    let consensus_state =
        get_consensus_state(deps.storage, timestamp_at_height_msg.height.revision_height)?;

    let nano_timestamp = consensus_state
        .timestamp
        .checked_mul(1_000_000_000) // ibc-go expects nanoseconds
        .ok_or(ContractError::TimestampOverflow)?;

    Ok(to_json_binary(&TimestampAtHeightResult {
        timestamp: nano_timestamp,
    })?)
}

/// Gets the status of the light client
/// # Returns
/// The current status of the client
/// # Errors
/// Errors if the client state can't be deserialized.
pub fn status(deps: Deps) -> Result<Binary, ContractError> {
    let client_state = get_client_state(deps.storage)?;

    if client_state.is_frozen {
        return Ok(to_json_binary(&StatusResult {
            status: Status::Frozen.to_string(),
        })?);
    }

    Ok(to_json_binary(&StatusResult {
        status: Status::Active.to_string(),
    })?)
}
