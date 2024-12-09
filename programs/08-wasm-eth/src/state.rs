use cosmwasm_std::Deps;
use ethereum_light_client::client_state::ClientState as EthClientState;
use ethereum_light_client::consensus_state::ConsensusState as EthConsensusState;
use ibc_proto::{
    google::protobuf::Any,
    ibc::lightclients::wasm::v1::{
        ClientState as WasmClientState, ConsensusState as WasmConsensusState,
    },
};
use prost::Message;

use crate::{custom_query::EthereumCustomQuery, msg::Height};

// Client state that is stored by the host
pub const HOST_CLIENT_STATE_KEY: &str = "clientState";
pub const HOST_CONSENSUS_STATES_KEY: &str = "consensusStates";

pub fn consensus_db_key(height: &Height) -> String {
    format!(
        "{}/{}-{}",
        HOST_CONSENSUS_STATES_KEY, height.revision_number, height.revision_height
    )
}

// TODO: Proper errors
pub fn get_eth_client_state(deps: Deps<EthereumCustomQuery>) -> EthClientState {
    let wasm_client_state_any_bz = deps.storage.get(HOST_CLIENT_STATE_KEY.as_bytes()).unwrap();
    let wasm_client_state_any = Any::decode(wasm_client_state_any_bz.as_slice()).unwrap();
    let wasm_client_state =
        WasmClientState::decode(wasm_client_state_any.value.as_slice()).unwrap();

    // TODO: map to ContractError
    serde_json::from_slice(&wasm_client_state.data).unwrap()
}

// TODO: Proper errors
pub fn get_eth_consensus_state(
    deps: Deps<EthereumCustomQuery>,
    height: &Height,
) -> EthConsensusState {
    let wasm_consensus_state_any_bz = deps
        .storage
        .get(consensus_db_key(height).as_bytes())
        .unwrap();
    let wasm_consensus_state_any = Any::decode(wasm_consensus_state_any_bz.as_slice()).unwrap();
    let wasm_consensus_state =
        WasmConsensusState::decode(wasm_consensus_state_any.value.as_slice()).unwrap();

    serde_json::from_slice(&wasm_consensus_state.data).unwrap()
}

/*
*             let actual_wasm_client_state_any_bz =
                deps.storage.get(HOST_CLIENT_STATE_KEY.as_bytes()).unwrap();
            let actual_wasm_client_state_any =
                Any::decode(actual_wasm_client_state_any_bz.as_slice()).unwrap();
            assert_eq!(
                WasmClientState::type_url(),
                actual_wasm_client_state_any.type_url
            );
            let actual_client_state =
                WasmClientState::decode(actual_wasm_client_state_any.value.as_slice()).unwrap();
            assert_eq!(msg.checksum, actual_client_state.checksum);
            assert_eq!(msg.client_state, actual_client_state.data);
            assert_eq!(
                0,
                actual_client_state.latest_height.unwrap().revision_number
            );
            assert_eq!(
                client_state.latest_slot,
                actual_client_state.latest_height.unwrap().revision_height
            );

            let actual_wasm_consensus_state_any_bz = deps
                .storage
                .get(
                    consensus_db_key(&Height {
                        revision_number: 0,
                        revision_height: consensus_state.slot,
                    })
                    .as_bytes(),
                )
                .unwrap();
            let actual_wasm_consensus_state_any =
                Any::decode(actual_wasm_consensus_state_any_bz.as_slice()).unwrap();
            assert_eq!(
                WasmConsensusState::type_url(),
                actual_wasm_consensus_state_any.type_url
            );
            let actual_consensus_state =
                WasmConsensusState::decode(actual_wasm_consensus_state_any.value.as_slice())
                    .unwrap();
*/
