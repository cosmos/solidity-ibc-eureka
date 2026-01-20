//! This module contains the instantiate helper functions

use attestor_light_client::{
    client_state::ClientState as AttestorClientState,
    consensus_state::ConsensusState as AttestorConsensusState,
};
use cosmwasm_std::{ensure, Storage};
use ibc_proto::ibc::{
    core::client::v1::Height as IbcProtoHeight,
    lightclients::wasm::v1::{
        ClientState as WasmClientState, ConsensusState as WasmConsensusState,
    },
};

use crate::{
    msg::InstantiateMsg,
    state::{store_client_state, store_consensus_state},
    ContractError,
};

/// Initializes the client state and initial consensus state
/// # Errors
/// Will return an error if the client state or consensus state cannot be deserialized.
/// # Panics
/// Will panic if the client state latest height cannot be unwrapped
#[allow(clippy::needless_pass_by_value)]
pub fn client(storage: &mut dyn Storage, msg: InstantiateMsg) -> Result<(), ContractError> {
    let client_state_bz: Vec<u8> = msg.client_state.into();
    let client_state: AttestorClientState = serde_json::from_slice(&client_state_bz)
        .map_err(ContractError::DeserializeClientStateFailed)?;
    let wasm_client_state = WasmClientState {
        checksum: msg.checksum.into(),
        data: client_state_bz,
        latest_height: Some(IbcProtoHeight {
            revision_number: 0,
            revision_height: client_state.latest_height,
        }),
    };

    let consensus_state_bz: Vec<u8> = msg.consensus_state.into();
    let consensus_state: AttestorConsensusState = serde_json::from_slice(&consensus_state_bz)
        .map_err(ContractError::DeserializeConsensusStateFailed)?;
    let wasm_consensus_state = WasmConsensusState {
        data: consensus_state_bz,
    };

    ensure!(
        wasm_client_state.latest_height.unwrap().revision_height == client_state.latest_height,
        ContractError::ClientStateHeightMismatch
    );

    ensure!(
        client_state.latest_height == consensus_state.height,
        ContractError::ClientAndConsensusStateMismatch
    );

    store_client_state(storage, &wasm_client_state)?;
    store_consensus_state(storage, &wasm_consensus_state, consensus_state.height)?;

    Ok(())
}
