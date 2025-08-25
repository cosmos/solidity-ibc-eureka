//! State management for the attestor light client

use attestor_light_client::client_state::ClientState;
use attestor_light_client::consensus_state::ConsensusState;
use cosmwasm_std::{Order, Storage};
use ibc_proto::{
    google::protobuf::Any,
    ibc::lightclients::wasm::v1::{
        ClientState as WasmClientState, ConsensusState as WasmConsensusState,
    },
};
use prost::Message;

use crate::ContractError;

/// The store key used by `ibc-go` to store the client state
pub const HOST_CLIENT_STATE_KEY: &str = "clientState";
/// The store key used by `ibc-go` to store the consensus states
pub const HOST_CONSENSUS_STATES_KEY: &str = "consensusStates";
/// The store key used by `ibc-go` to store sorted keys of consensusStates
pub const HOST_ITERATE_CONSENSUS_STATES_KEY: &str = "iterateConsensusStates";

/// The key used to store the consensus states by height
#[must_use]
pub fn consensus_db_key(height: u64) -> String {
    format!("{}/{}-{}", HOST_CONSENSUS_STATES_KEY, 0, height)
}

fn height_to_big_endian(revision: u64, height: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(16);
    key.extend_from_slice(&revision.to_be_bytes());
    key.extend_from_slice(&height.to_be_bytes());
    key
}

/// The key used to store the consensus states by height
#[must_use]
pub fn iteration_db_key(revision: u64, height: u64) -> Vec<u8> {
    let key = height_to_big_endian(revision, height);
    let mut prefix = format!("{HOST_ITERATE_CONSENSUS_STATES_KEY}/")
        .as_bytes()
        .to_vec();
    prefix.extend(key);
    prefix
}

/// Get the Wasm client state
/// # Errors
/// Returns an error if the client state is not found or cannot be deserialized
/// # Returns
/// The Wasm client state
pub fn get_wasm_client_state(storage: &dyn Storage) -> Result<WasmClientState, ContractError> {
    let wasm_client_state_any_bz = storage
        .get(HOST_CLIENT_STATE_KEY.as_bytes())
        .ok_or(ContractError::ClientStateNotFound)?;
    let wasm_client_state_any = Any::decode(wasm_client_state_any_bz.as_slice())?;

    Ok(WasmClientState::decode(
        wasm_client_state_any.value.as_slice(),
    )?)
}

/// Get the attestor client state
/// # Errors
/// Returns an error if the client state is not found or cannot be deserialized
/// # Returns
/// The attestor client state
pub fn get_client_state(storage: &dyn Storage) -> Result<ClientState, ContractError> {
    let wasm_client_state = get_wasm_client_state(storage)?;
    Ok(serde_json::from_slice(&wasm_client_state.data)?)
}

/// Get the attestor consensus state at a given height
/// # Errors
/// Returns an error if the consensus state is not found or cannot be deserialized
/// # Returns
/// The attestor consensus state
pub fn get_consensus_state(
    storage: &dyn Storage,
    height: u64,
) -> Result<ConsensusState, ContractError> {
    let wasm_consensus_state_any_bz = storage
        .get(consensus_db_key(height).as_bytes())
        .ok_or(ContractError::ConsensusStateNotFound)?;
    let wasm_consensus_state_any = Any::decode(wasm_consensus_state_any_bz.as_slice())?;
    let wasm_consensus_state =
        WasmConsensusState::decode(wasm_consensus_state_any.value.as_slice())?;

    Ok(serde_json::from_slice(&wasm_consensus_state.data)?)
}

/// Get the previous attestor consensus state at a given height
/// # Errors
/// Returns an error if the consensus state is not found or cannot be deserialized
/// # Returns
/// The attestor previous consensus state
pub fn get_previous_consensus_state(
    storage: &dyn Storage,
    height: u64,
) -> Result<Option<ConsensusState>, ContractError> {
    let target_key = iteration_db_key(0, height);

    let mut prev_key = storage.range(None, Some(&target_key), Order::Descending);

    if let Some((_, consensus_key_bytes)) = prev_key.next() {
        let consensus_key = String::from_utf8_lossy(&consensus_key_bytes);
        let bytes = storage
            .get(consensus_key.as_bytes())
            .ok_or(ContractError::ConsensusStateNotFound)?;

        let any = Any::decode(bytes.as_slice())?;
        let wasm_consensus_state = WasmConsensusState::decode(any.value.as_slice())?;

        Ok(Some(serde_json::from_slice(&wasm_consensus_state.data)?))
    } else {
        Ok(None)
    }
}

/// Get the next attestor consensus state at a given height
/// # Errors
/// Returns an error if the consensus state is not found or cannot be deserialized
/// # Returns
/// The attestor next consensus state
pub fn get_next_consensus_state(
    storage: &dyn Storage,
    height: u64,
) -> Result<Option<ConsensusState>, ContractError> {
    let target_key = iteration_db_key(0, height);

    let mut next_key = storage.range(Some(&target_key), None, Order::Ascending);

    if let Some((_, consensus_key_bytes)) = next_key.next() {
        let consensus_key = String::from_utf8_lossy(&consensus_key_bytes);
        let bytes = storage
            .get(consensus_key.as_bytes())
            .ok_or(ContractError::ConsensusStateNotFound)?;

        let any = Any::decode(bytes.as_slice())?;
        let wasm_consensus_state = WasmConsensusState::decode(any.value.as_slice())?;

        Ok(Some(serde_json::from_slice(&wasm_consensus_state.data)?))
    } else {
        Ok(None)
    }
}

/// Store the consensus state
/// # Errors
/// Returns an error if the consensus state cannot be serialized into an Any
pub fn store_consensus_state(
    storage: &mut dyn Storage,
    wasm_consensus_state: &WasmConsensusState,
    height: u64,
) -> Result<(), ContractError> {
    let consensus_key = consensus_db_key(height);
    let wasm_consensus_state_any = Any::from_msg(wasm_consensus_state)?;
    storage.set(
        consensus_key.as_bytes(),
        wasm_consensus_state_any.encode_to_vec().as_slice(),
    );

    let iteration_key = iteration_db_key(0, height);
    storage.set(&iteration_key, consensus_key.as_bytes());

    Ok(())
}

/// Store the client state
/// # Errors
/// Returns an error if the client state cannot be serialized into an Any
#[allow(clippy::module_name_repetitions)]
pub fn store_client_state(
    storage: &mut dyn Storage,
    wasm_client_state: &WasmClientState,
) -> Result<(), ContractError> {
    let wasm_client_state_any = Any::from_msg(wasm_client_state)?;
    storage.set(
        HOST_CLIENT_STATE_KEY.as_bytes(),
        wasm_client_state_any.encode_to_vec().as_slice(),
    );

    Ok(())
}
