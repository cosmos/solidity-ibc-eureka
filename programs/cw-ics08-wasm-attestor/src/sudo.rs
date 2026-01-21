//! This module contains the sudo message handlers

use attestor_light_client::{header::Header, update::update_consensus_state};
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut};
use ibc_proto::ibc::{
    core::client::v1::Height as IbcProtoHeight,
    lightclients::wasm::v1::ConsensusState as WasmConsensusState,
};

// Keep module compiled without direct usage

use crate::{
    msg::{
        Height, UpdateStateMsg, UpdateStateOnMisbehaviourMsg, UpdateStateResult,
        VerifyMembershipMsg, VerifyNonMembershipMsg,
    },
    state::{
        get_client_state, get_consensus_state, get_wasm_client_state, store_client_state,
        store_consensus_state,
    },
    ContractError,
};

/// Verify the membership of a value at a given height
/// # Errors
/// Returns an error if the membership proof verification fails
/// # Returns
/// An empty response
#[allow(clippy::needless_pass_by_value)]
pub fn verify_membership(
    deps: Deps,
    verify_membership_msg: VerifyMembershipMsg,
) -> Result<Binary, ContractError> {
    let client_state = get_client_state(deps.storage)?;
    let consensus_state =
        get_consensus_state(deps.storage, verify_membership_msg.height.revision_height)?;

    attestor_light_client::membership::verify_membership(
        &consensus_state,
        &client_state,
        verify_membership_msg.proof.into(),
        verify_membership_msg
            .merkle_path
            .key_path
            .into_iter()
            .map(Into::into)
            .collect(),
        verify_membership_msg.value.into(),
    )
    .map_err(ContractError::VerifyMembershipFailed)?;

    Ok(Binary::default())
}

/// Verify the non-membership (absence) of a value at a given height
/// Used for timeout proofs where we verify the receipt commitment is ZERO
/// # Errors
/// Returns an error if the non-membership proof verification fails
/// # Returns
/// An empty response
#[allow(clippy::needless_pass_by_value)]
pub fn verify_non_membership(
    deps: Deps,
    verify_non_membership_msg: VerifyNonMembershipMsg,
) -> Result<Binary, ContractError> {
    let client_state = get_client_state(deps.storage)?;
    let consensus_state = get_consensus_state(
        deps.storage,
        verify_non_membership_msg.height.revision_height,
    )?;

    attestor_light_client::membership::verify_non_membership(
        &consensus_state,
        &client_state,
        verify_non_membership_msg.proof.into(),
        verify_non_membership_msg
            .merkle_path
            .key_path
            .into_iter()
            .map(Into::into)
            .collect(),
    )
    .map_err(ContractError::VerifyNonMembershipFailed)?;

    Ok(Binary::default())
}

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
    let header: Header = serde_json::from_slice(&header_bz)
        .map_err(ContractError::DeserializeClientMessageFailed)?;

    let client_state = get_client_state(deps.storage)?;

    let (updated_slot, updated_consensus_state, updated_client_state) =
        update_consensus_state(client_state, &header)
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
    let mut client_state = get_client_state(deps.storage)?;
    client_state.is_frozen = true;

    let client_state_bz: Vec<u8> =
        serde_json::to_vec(&client_state).map_err(ContractError::SerializeClientStateFailed)?;

    let mut wasm_client_state = get_wasm_client_state(deps.storage)?;
    wasm_client_state.data = client_state_bz;

    store_client_state(deps.storage, &wasm_client_state)?;

    Ok(Binary::default())
}
