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
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let result = match msg {
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
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
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
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
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

#[cfg(test)]
mod tests {
    mod instantiate {
        use cosmwasm_std::{
            coins,
            testing::{message_info, mock_env},
            Storage,
        };
        use ibc_proto::{
            google::protobuf::Any,
            ibc::lightclients::wasm::v1::{
                ClientState as WasmClientState, ConsensusState as WasmConsensusState,
            },
        };
        use prost::{Message, Name};
        use solana_light_client::{
            client_state::ClientState as SolClientState,
            consensus_state::ConsensusState as SolConsensusState,
        };
        use solana_types::consensus::fork::ForkParameters;

        use crate::{
            contract::instantiate,
            msg::InstantiateMsg,
            state::{consensus_db_key, HOST_CLIENT_STATE_KEY},
            test::helpers::mk_deps,
        };

        #[test]
        fn assigns_correct_values() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = SolClientState {
                latest_slot: 42,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();

            let consensus_state = SolConsensusState {
                slot: 42,
                timestamp: 1234567890,
            };
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: client_state_bz.into(),
                consensus_state: consensus_state_bz.into(),
                checksum: b"solana_checksum".into(),
            };

            let res = instantiate(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();
            assert_eq!(0, res.messages.len());

            // Verify client state storage
            let actual_wasm_client_state_any_bz =
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

            // Verify consensus state storage
            let actual_wasm_consensus_state_any_bz = deps
                .storage
                .get(consensus_db_key(consensus_state.slot).as_bytes())
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
            assert_eq!(msg.consensus_state, actual_consensus_state.data);
        }
    }

    mod integration_tests {
        use cosmwasm_std::{
            coins,
            testing::{message_info, mock_env},
            Binary, Storage, Timestamp,
        };
        use ibc_proto::{
            google::protobuf::Any, ibc::lightclients::wasm::v1::ClientState as WasmClientState,
        };
        use prost::Message;
        use solana_light_client::{
            client_state::ClientState as SolClientState,
            consensus_state::ConsensusState as SolConsensusState, error::SolanaIBCError,
            header::Header,
        };
        use solana_types::consensus::fork::ForkParameters;

        use crate::{
            contract::{instantiate, migrate, query, sudo},
            msg::{
                InstantiateMsg, MigrateMsg, Migration, QueryMsg, SudoMsg, UpdateStateMsg,
                UpdateStateResult, VerifyClientMessageMsg,
            },
            state::HOST_CLIENT_STATE_KEY,
            test::helpers::mk_deps,
            ContractError,
        };

        #[test]
        fn basic_client_update_flow() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = SolClientState {
                latest_slot: 100,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let consensus_state = SolConsensusState {
                slot: 100,
                timestamp: 1234567890,
            };

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: Binary::from(client_state_bz),
                consensus_state: Binary::from(consensus_state_bz),
                checksum: b"checksum".into(),
            };

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Create a header for client update (slot progression)
            let header = Header {
                trusted_slot: 100,
                new_slot: 150,
                timestamp: 1234567900,            // 10 seconds later
                signature_data: vec![1, 2, 3, 4], // Some signature data
            };
            let header_bz = serde_json::to_vec(&header).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(header.timestamp + 100);

            // Verify client message
            let query_verify_client_msg = QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                client_message: Binary::from(header_bz.clone()),
            });
            query(deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();

            // Update state
            let sudo_update_state_msg = SudoMsg::UpdateState(UpdateStateMsg {
                client_message: Binary::from(header_bz),
            });
            let update_res = sudo(deps.as_mut(), env.clone(), sudo_update_state_msg).unwrap();
            let update_state_result: UpdateStateResult =
                serde_json::from_slice(&update_res.data.unwrap())
                    .expect("update state result should be deserializable");

            assert_eq!(1, update_state_result.heights.len());
            assert_eq!(0, update_state_result.heights[0].revision_number);
            assert_eq!(
                header.new_slot,
                update_state_result.heights[0].revision_height
            );
        }

        #[test]
        fn invalid_slot_regression() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = SolClientState {
                latest_slot: 100,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let consensus_state = SolConsensusState {
                slot: 100,
                timestamp: 1234567890,
            };

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: Binary::from(client_state_bz),
                consensus_state: Binary::from(consensus_state_bz),
                checksum: b"checksum".into(),
            };

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Create a header with slot regression (new_slot < trusted_slot)
            let header = Header {
                trusted_slot: 100,
                new_slot: 90, // This should fail
                timestamp: 1234567900,
                signature_data: vec![1, 2, 3, 4],
            };
            let header_bz = serde_json::to_vec(&header).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(header.timestamp + 100);

            // Verify client message should fail
            let query_verify_client_msg = QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                client_message: Binary::from(header_bz),
            });
            let err = query(deps.as_ref(), env, query_verify_client_msg).unwrap_err();
            assert!(matches!(
                err,
                ContractError::VerifyClientMessageFailed(
                    SolanaIBCError::InvalidSlotProgression { .. }
                )
            ));
        }

        #[test]
        fn missing_signature_data() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = SolClientState {
                latest_slot: 100,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let consensus_state = SolConsensusState {
                slot: 100,
                timestamp: 1234567890,
            };

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: Binary::from(client_state_bz),
                consensus_state: Binary::from(consensus_state_bz),
                checksum: b"checksum".into(),
            };

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Create a header with empty signature data
            let header = Header {
                trusted_slot: 100,
                new_slot: 150,
                timestamp: 1234567900,
                signature_data: vec![], // Empty signature data should fail
            };
            let header_bz = serde_json::to_vec(&header).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(header.timestamp + 100);

            // Verify client message should fail
            let query_verify_client_msg = QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                client_message: Binary::from(header_bz),
            });
            let err = query(deps.as_ref(), env, query_verify_client_msg).unwrap_err();
            println!("{:#?}", err);
            assert!(matches!(
                err,
                ContractError::VerifyClientMessageFailed(SolanaIBCError::InvalidSignature)
            ));
        }

        #[test]
        fn frozen_client() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup frozen client state
            let client_state = SolClientState {
                latest_slot: 100,
                fork_parameters: ForkParameters,
                is_frozen: true, // Client is frozen
            };
            let consensus_state = SolConsensusState {
                slot: 100,
                timestamp: 1234567890,
            };

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: Binary::from(client_state_bz),
                consensus_state: Binary::from(consensus_state_bz),
                checksum: b"checksum".into(),
            };

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Create a valid header
            let header = Header {
                trusted_slot: 100,
                new_slot: 150,
                timestamp: 1234567900,
                signature_data: vec![1, 2, 3, 4],
            };
            let header_bz = serde_json::to_vec(&header).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(header.timestamp + 100);

            // Verify client message should fail because client is frozen
            let query_verify_client_msg = QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                client_message: Binary::from(header_bz),
            });
            let err = query(deps.as_ref(), env, query_verify_client_msg).unwrap_err();
            assert!(matches!(
                err,
                ContractError::VerifyClientMessageFailed(SolanaIBCError::ClientFrozen)
            ));
        }

        #[test]
        fn migrate_with_same_state_version() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = SolClientState {
                latest_slot: 42,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let consensus_state = SolConsensusState {
                slot: 42,
                timestamp: 1234567890,
            };

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: Binary::from(client_state_bz),
                consensus_state: Binary::from(consensus_state_bz),
                checksum: b"checksum".into(),
            };

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Migrate without any changes (i.e. same state version)
            migrate(
                deps.as_mut(),
                mock_env(),
                MigrateMsg {
                    migration: Migration::CodeOnly,
                },
            )
            .unwrap();
        }

        #[test]
        fn migrate_with_reinstantiate() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = SolClientState {
                latest_slot: 42,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let consensus_state = SolConsensusState {
                slot: 42,
                timestamp: 1234567890,
            };

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: client_state_bz.into(),
                consensus_state: consensus_state_bz.into(),
                checksum: b"original_checksum".into(),
            };

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Create new state for migration
            let new_client_state = SolClientState {
                latest_slot: 100,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let new_consensus_state = SolConsensusState {
                slot: 100,
                timestamp: 1234567999,
            };

            let new_client_state_bz: Vec<u8> = serde_json::to_vec(&new_client_state).unwrap();
            let new_consensus_state_bz: Vec<u8> = serde_json::to_vec(&new_consensus_state).unwrap();

            let new_msg = InstantiateMsg {
                client_state: Binary::from(new_client_state_bz),
                consensus_state: Binary::from(new_consensus_state_bz),
                checksum: b"new_checksum".into(),
            };

            let migrate_msg = MigrateMsg {
                migration: Migration::Reinstantiate(new_msg.clone()),
            };

            // Migrate with reinstantiation
            migrate(deps.as_mut(), mock_env(), migrate_msg).unwrap();

            // Verify the new state is stored
            let actual_wasm_client_state_any_bz =
                deps.storage.get(HOST_CLIENT_STATE_KEY.as_bytes()).unwrap();
            let actual_wasm_client_state_any =
                Any::decode(actual_wasm_client_state_any_bz.as_slice()).unwrap();
            let wasm_client_state =
                WasmClientState::decode(actual_wasm_client_state_any.value.as_slice()).unwrap();
            assert_eq!(new_msg.checksum, wasm_client_state.checksum);
            assert_eq!(
                wasm_client_state.latest_height.unwrap().revision_height,
                new_client_state.latest_slot
            );
        }

        #[test]
        fn migrate_with_fork_parameters() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = SolClientState {
                latest_slot: 42,
                fork_parameters: ForkParameters,
                is_frozen: false,
            };
            let consensus_state = SolConsensusState {
                slot: 42,
                timestamp: 1234567890,
            };

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: client_state_bz.into(),
                consensus_state: consensus_state_bz.into(),
                checksum: b"checksum".into(),
            };
            let msg_copy = msg.clone();

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            let migrate_msg = MigrateMsg {
                migration: Migration::UpdateForkParameters(ForkParameters),
            };

            // Migrate with fork parameter update
            migrate(deps.as_mut(), mock_env(), migrate_msg).unwrap();

            let actual_wasm_client_state_any_bz =
                deps.storage.get(HOST_CLIENT_STATE_KEY.as_bytes()).unwrap();
            let actual_wasm_client_state_any =
                Any::decode(actual_wasm_client_state_any_bz.as_slice()).unwrap();
            let wasm_client_state =
                WasmClientState::decode(actual_wasm_client_state_any.value.as_slice()).unwrap();

            // Verify checksum hasn't changed
            assert_eq!(msg_copy.checksum, wasm_client_state.checksum);
            // Verify latest height hasn't changed
            assert_eq!(
                wasm_client_state.latest_height.unwrap().revision_height,
                client_state.latest_slot
            );

            // Verify we can deserialize the updated client state
            let sol_client_state: SolClientState =
                serde_json::from_slice(&wasm_client_state.data).unwrap();
            assert_eq!(sol_client_state.latest_slot, client_state.latest_slot);
            assert_eq!(sol_client_state.fork_parameters, ForkParameters);
        }
    }
}
