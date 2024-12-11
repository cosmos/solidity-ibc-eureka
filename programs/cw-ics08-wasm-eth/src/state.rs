//! State management for the Ethereum light client

use cosmwasm_std::Storage;
use ethereum_light_client::client_state::ClientState as EthClientState;
use ethereum_light_client::consensus_state::ConsensusState as EthConsensusState;
use ibc_proto::{
    google::protobuf::Any,
    ibc::lightclients::wasm::v1::{
        ClientState as WasmClientState, ConsensusState as WasmConsensusState,
    },
};
use prost::Message;

use crate::msg::Height;

/// The store key used by `ibc-go` to store the client state
pub const HOST_CLIENT_STATE_KEY: &str = "clientState";
/// The store key used by `ibc-go` to store the consensus states
pub const HOST_CONSENSUS_STATES_KEY: &str = "consensusStates";

/// The key used to store the consensus states by height
#[must_use]
pub fn consensus_db_key(height: &Height) -> String {
    format!(
        "{}/{}-{}",
        HOST_CONSENSUS_STATES_KEY, height.revision_number, height.revision_height
    )
}

// TODO: Proper errors
/// Get the Ethereum client state
/// # Panics
/// Panics if the client state is not found or cannot be deserialized
#[allow(clippy::module_name_repetitions)]
pub fn get_eth_client_state(storage: &dyn Storage) -> EthClientState {
    let wasm_client_state_any_bz = storage.get(HOST_CLIENT_STATE_KEY.as_bytes()).unwrap();
    let wasm_client_state_any = Any::decode(wasm_client_state_any_bz.as_slice()).unwrap();
    let wasm_client_state =
        WasmClientState::decode(wasm_client_state_any.value.as_slice()).unwrap();

    // TODO: map to ContractError
    serde_json::from_slice(&wasm_client_state.data).unwrap()
}

// TODO: Proper errors
/// Get the Ethereum consensus state at a given height
/// # Panics
/// Panics if the consensus state is not found or cannot be deserialized
#[allow(clippy::module_name_repetitions)]
pub fn get_eth_consensus_state(storage: &dyn Storage, height: &Height) -> EthConsensusState {
    let wasm_consensus_state_any_bz = storage.get(consensus_db_key(height).as_bytes()).unwrap();
    let wasm_consensus_state_any = Any::decode(wasm_consensus_state_any_bz.as_slice()).unwrap();
    let wasm_consensus_state =
        WasmConsensusState::decode(wasm_consensus_state_any.value.as_slice()).unwrap();

    serde_json::from_slice(&wasm_consensus_state.data).unwrap()
}
