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
    let sol_client_state = get_client_state(deps.storage)?;

    if let Ok(header) = serde_json::from_slice::<Header>(&verify_client_message_msg.client_message)
    {
        if let Ok(height_in_msg_exists) = get_consensus_state(deps.storage, header.new_height) {
            let _sol_consensus_state = attestor_light_client::verify::verify_header(
                Some(&height_in_msg_exists),
                None,
                None,
                &sol_client_state,
                &header,
            )
            .map_err(ContractError::VerifyClientMessageFailed)?;

            return Ok(Binary::default());
        }

        let (prev, next) = (
            get_previous_consensus_state(deps.storage, header.new_height)?,
            get_next_consensus_state(deps.storage, header.new_height)?,
        );
        let _sol_consensus_state = attestor_light_client::verify::verify_header(
            None,
            prev.as_ref(),
            next.as_ref(),
            &sol_client_state,
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
/// Returns an error if the conensus state is not found
/// # Returns
/// The timestamp at the given height
#[allow(clippy::needless_pass_by_value)]
pub fn timestamp_at_height(
    deps: Deps,
    timestamp_at_height_msg: TimestampAtHeightMsg,
) -> Result<Binary, ContractError> {
    let sol_consensus_state =
        get_consensus_state(deps.storage, timestamp_at_height_msg.height.revision_height)?;

    let nano_timestamp = sol_consensus_state.timestamp * 1_000_000_000; // ibc-go expects nanoseconds

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
    let sol_client_state = get_client_state(deps.storage)?;

    if sol_client_state.is_frozen {
        return Ok(to_json_binary(&StatusResult {
            status: Status::Frozen.to_string(),
        })?);
    }

    Ok(to_json_binary(&StatusResult {
        status: Status::Active.to_string(),
    })?)
}
