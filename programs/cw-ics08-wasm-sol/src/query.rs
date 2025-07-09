//! This module contains the query message handlers

use cosmwasm_std::{to_json_binary, Binary, Deps, Env};
use solana_light_client::header::Header;

use crate::{
    msg::{
        CheckForMisbehaviourMsg, CheckForMisbehaviourResult, SolanaMisbehaviourMsg, Status,
        StatusResult, TimestampAtHeightMsg, TimestampAtHeightResult, VerifyClientMessageMsg,
    },
    state::{get_sol_client_state, get_sol_consensus_state},
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
    env: Env,
    verify_client_message_msg: VerifyClientMessageMsg,
) -> Result<Binary, ContractError> {
    let sol_client_state = get_sol_client_state(deps.storage)?;

    if let Ok(header) = serde_json::from_slice::<Header>(&verify_client_message_msg.client_message)
    {
        let sol_consensus_state = get_sol_consensus_state(deps.storage, header.trusted_slot)?;

        solana_light_client::verify::verify_header(
            &sol_consensus_state,
            &sol_client_state,
            env.block.time.seconds(),
            &header,
        )
        .map_err(ContractError::VerifyClientMessageFailed)?;

        return Ok(Binary::default());
    }

    if let Ok(misbehaviour) =
        serde_json::from_slice::<SolanaMisbehaviourMsg>(&verify_client_message_msg.client_message)
    {
        let sol_consensus_state = get_sol_consensus_state(deps.storage, misbehaviour.trusted_slot)?;

        solana_light_client::misbehaviour::verify_misbehaviour(
            &sol_client_state,
            &sol_consensus_state,
            &misbehaviour.sync_committee,
            &misbehaviour.update_1,
            &misbehaviour.update_2,
            env.block.time.seconds(),
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
    deps: Deps,
    env: Env,
    check_for_misbehaviour_msg: CheckForMisbehaviourMsg,
) -> Result<Binary, ContractError> {
    let misbehaviour = serde_json::from_slice::<SolanaMisbehaviourMsg>(
        &check_for_misbehaviour_msg.client_message,
    )
    .map_err(ContractError::DeserializeSolMisbehaviourFailed)?;

    let sol_client_state = get_sol_client_state(deps.storage)?;
    let sol_consensus_state = get_sol_consensus_state(deps.storage, misbehaviour.trusted_slot)?;

    solana_light_client::misbehaviour::verify_misbehaviour(
        &sol_client_state,
        &sol_consensus_state,
        &misbehaviour.sync_committee,
        &misbehaviour.update_1,
        &misbehaviour.update_2,
        env.block.time.seconds(),
    )
    .map_err(ContractError::VerifyClientMessageFailed)?;

    Ok(to_json_binary(&CheckForMisbehaviourResult {
        found_misbehaviour: true,
    })?)
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
        get_sol_consensus_state(deps.storage, timestamp_at_height_msg.height.revision_height)?;

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
    let sol_client_state = get_sol_client_state(deps.storage)?;

    if sol_client_state.is_frozen {
        return Ok(to_json_binary(&StatusResult {
            status: Status::Frozen.to_string(),
        })?);
    }

    Ok(to_json_binary(&StatusResult {
        status: Status::Active.to_string(),
    })?)
}
