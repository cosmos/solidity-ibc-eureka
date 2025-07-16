//! This module contains the sudo message handlers

use cosmwasm_std::{to_json_binary, Binary, DepsMut};
use ibc_proto::ibc::{
    core::client::v1::Height as IbcProtoHeight,
    lightclients::wasm::v1::ConsensusState as WasmConsensusState,
};
use solana_light_client::update::update_consensus_state;

use crate::{
    msg::{Height, UpdateStateMsg, UpdateStateOnMisbehaviourMsg, UpdateStateResult},
    state::{
        get_attestor_client_state, get_wasm_client_state, store_client_state, store_consensus_state,
    },
    ContractError,
};

/// Update the state of the light client
/// This function is always called after the verify client message, so
/// we can assume the client message is valid and that the consensus state can be updated
/// # Errors
/// Returns an error if deserialization failes or if the light client update logic fails
/// # Returns
/// The updated slot (called height in regular IBC terms)
#[allow(clippy::needless_pass_by_value)]
pub fn update_state(
    deps: DepsMut,
    update_state_msg: UpdateStateMsg,
) -> Result<Binary, ContractError> {
    let header_bz: Vec<u8> = update_state_msg.client_message.into();
    let header = serde_json::from_slice(&header_bz)
        .map_err(ContractError::DeserializeClientMessageFailed)?;

    let sol_client_state = get_attestor_client_state(deps.storage)?;

    let (updated_slot, updated_consensus_state, updated_client_state) =
        update_consensus_state(sol_client_state, header)
            .map_err(ContractError::UpdateClientStateFailed)?;

    let consensus_state_bz: Vec<u8> = serde_json::to_vec(&updated_consensus_state)
        .map_err(ContractError::SerializeConsensusStateFailed)?;
    let wasm_consensus_state = WasmConsensusState {
        data: consensus_state_bz,
    };

    store_consensus_state(deps.storage, &wasm_consensus_state, updated_slot)?;
    if let Some(client_state) = updated_client_state {
        let client_state_bz: Vec<u8> =
            serde_json::to_vec(&client_state).map_err(ContractError::SerializeClientStateFailed)?;

        let mut wasm_client_state = get_wasm_client_state(deps.storage)?;
        wasm_client_state.data = client_state_bz;
        wasm_client_state.latest_height = Some(IbcProtoHeight {
            revision_number: 0,
            revision_height: updated_slot,
        });
        store_client_state(deps.storage, &wasm_client_state)?;
    }

    Ok(to_json_binary(&UpdateStateResult {
        heights: vec![Height {
            revision_number: 0,
            revision_height: updated_slot,
        }],
    })?)
}

/// Update the state of the light client on misbehaviour
/// # Errors
/// Returns an error if the misbehaviour verification fails
#[allow(clippy::needless_pass_by_value)]
pub fn misbehaviour(
    deps: DepsMut,
    _msg: UpdateStateOnMisbehaviourMsg,
) -> Result<Binary, ContractError> {
    let mut sol_client_state = get_attestor_client_state(deps.storage)?;
    sol_client_state.is_frozen = true;

    let client_state_bz: Vec<u8> =
        serde_json::to_vec(&sol_client_state).map_err(ContractError::SerializeClientStateFailed)?;

    let mut wasm_client_state = get_wasm_client_state(deps.storage)?;
    wasm_client_state.data = client_state_bz;

    store_client_state(deps.storage, &wasm_client_state)?;

    Ok(Binary::default())
}
