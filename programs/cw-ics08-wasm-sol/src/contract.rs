//! This module contains the `CosmWasm` entrypoints for the 08-wasm smart contract

use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, Migration, QueryMsg, SudoMsg};
use crate::{instantiate, query, state};
use crate::{sudo, ContractError};

/// The version of the contracts state.
/// It is used to determine if the state needs to be migrated in the migrate entry point.
const STATE_VERSION: &str = env!("CARGO_PKG_VERSION");
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");

/// The instantiate entry point for the CosmWasm contract.
/// # Errors
/// Will return an error if the client state or consensus state cannot be deserialized.
/// # Panics
/// Will panic if the client state latest height cannot be unwrapped
#[entry_point]
#[allow(clippy::needless_pass_by_value)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, STATE_VERSION)?;

    instantiate::client(deps.storage, msg)?;

    Ok(Response::default())
}

/// The sudo entry point for the CosmWasm contract.
/// It routes the message to the appropriate handler.
/// # Errors
/// Will return an error if the handler returns an error.
#[entry_point]
#[allow(clippy::needless_pass_by_value)]
pub fn sudo(
    deps: DepsMut,
    _env: Env,
    msg: SudoMsg,
) -> Result<Response, ContractError> {
    let result = match msg {
        SudoMsg::VerifyMembership(verify_membership_msg) => {
            sudo::verify_membership(deps.as_ref(), verify_membership_msg)?
        }
        SudoMsg::VerifyNonMembership(verify_non_membership_msg) => {
            sudo::verify_non_membership(deps.as_ref(), verify_non_membership_msg)?
        }
        SudoMsg::UpdateState(update_state_msg) => sudo::update_state(deps, update_state_msg)?,
        SudoMsg::UpdateStateOnMisbehaviour(misbehaviour_msg) => {
            sudo::misbehaviour(deps, misbehaviour_msg)?
        }
        SudoMsg::VerifyUpgradeAndUpdateState(_) => todo!(),
        SudoMsg::MigrateClientStore(_) => todo!(),
    };

    Ok(Response::default().set_data(result))
}

/// Execute entry point is not used in this contract.
#[entry_point]
#[allow(clippy::needless_pass_by_value, clippy::missing_errors_doc)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    unimplemented!()
}

/// The query entry point for the CosmWasm contract.
/// It routes the message to the appropriate handler.
/// # Errors
/// Will return an error if the handler returns an error.
#[entry_point]
pub fn query(
    deps: Deps,
    env: Env,
    msg: QueryMsg,
) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::VerifyClientMessage(verify_client_message_msg) => {
            query::verify_client_message(deps, env, verify_client_message_msg)
        }
        QueryMsg::CheckForMisbehaviour(check_for_misbehaviour_msg) => {
            query::check_for_misbehaviour(deps, env, check_for_misbehaviour_msg)
        }
        QueryMsg::TimestampAtHeight(timestamp_at_height_msg) => {
            query::timestamp_at_height(deps, timestamp_at_height_msg)
        }
        QueryMsg::Status(_) => query::status(deps),
    }
}

/// The migrate entry point for the CosmWasm contract.
/// # Errors
/// Will return an errror if the state version is not newer than the current one.
#[entry_point]
#[allow(clippy::needless_pass_by_value)]
pub fn migrate(
    deps: DepsMut,
    _env: Env,
    msg: MigrateMsg,
) -> Result<Response, ContractError> {
    // Check if the state version is older than the current one and update it
    cw2::ensure_from_older_version(deps.storage, CONTRACT_NAME, STATE_VERSION)?;

    // Perform the migration
    match msg.migration {
        Migration::CodeOnly => {} // do nothing here
        Migration::Reinstantiate(instantiate_msg) => {
            // Re-instantiate the client
            instantiate::client(deps.storage, instantiate_msg)?;
        }
        Migration::UpdateForkParameters(fork_parameters) => {
            // Change the fork parameters
            let mut client_state = state::get_sol_client_state(deps.storage)?;
            client_state.fork_parameters = fork_parameters;
            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state)
                .map_err(ContractError::SerializeClientStateFailed)?;
            let mut wasm_client_state = state::get_wasm_client_state(deps.storage)?;
            wasm_client_state.data = client_state_bz;
            state::store_client_state(deps.storage, &wasm_client_state)?;
        }
    }

    Ok(Response::default())
}
