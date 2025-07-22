//! This module contains the `CosmWasm` entrypoints for the 08-wasm smart contract

use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, SudoMsg};
use crate::{instantiate, query};
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
        SudoMsg::UpdateStateOnMisbehaviour(_) => {
            todo!()
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
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::VerifyClientMessage(verify_client_message_msg) => {
            query::verify_client_message(deps, verify_client_message_msg)
        }
        QueryMsg::CheckForMisbehaviour(_) => {
            todo!()
        }
        QueryMsg::TimestampAtHeight(_) => {
            todo!()
        }
        QueryMsg::Status(_) => query::status(deps),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use secp256k1::{ecdsa::Signature, hashes::Hash, Message, PublicKey, SecretKey};
    use std::cell::LazyCell;

    pub const DUMMY_DATA: [u8; 1] = [0];
    pub const S_KEYS: LazyCell<[SecretKey; 5]> = LazyCell::new(|| {
        [
            SecretKey::from_byte_array([0xcd; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x02; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x03; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x10; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x1F; 32]).expect("32 bytes, within curve order"),
        ]
    });
    pub const KEYS: LazyCell<[PublicKey; 5]> = LazyCell::new(|| {
        [
            PublicKey::from_secret_key_global(&S_KEYS[0]),
            PublicKey::from_secret_key_global(&S_KEYS[1]),
            PublicKey::from_secret_key_global(&S_KEYS[2]),
            PublicKey::from_secret_key_global(&S_KEYS[3]),
            PublicKey::from_secret_key_global(&S_KEYS[4]),
        ]
    });
    pub const SIGS: LazyCell<Vec<Signature>> = LazyCell::new(|| {
        let sigs = S_KEYS
            .iter()
            .map(|skey| {
                let digest = secp256k1::hashes::sha256::Hash::hash(&DUMMY_DATA);
                let message = Message::from_digest(digest.to_byte_array());
                skey.sign_ecdsa(message)
            })
            .collect();

        sigs
    });

    mod instantiate {

        use attestor_light_client::{
            client_state::ClientState as AttestorClientState,
            consensus_state::ConsensusState as AttestorConsensusState,
        };
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

        use crate::{
            contract::{
                instantiate,
                tests::{DUMMY_DATA, KEYS},
            },
            msg::InstantiateMsg,
            state::{consensus_db_key, HOST_CLIENT_STATE_KEY},
            test::helpers::mk_deps,
        };

        #[test]
        fn assigns_correct_values() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 42,
                is_frozen: false,
                min_required_sigs: 5,
            };
            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();

            let consensus_state = AttestorConsensusState {
                height: 42,
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
                client_state.latest_height,
                actual_client_state.latest_height.unwrap().revision_height
            );

            // Verify consensus state storage
            let actual_wasm_consensus_state_any_bz = deps
                .storage
                .get(consensus_db_key(consensus_state.height).as_bytes())
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
        use attestor_light_client::{
            client_state::ClientState as AttestorClientState,
            consensus_state::ConsensusState as AttestorConsensusState, error::SolanaIBCError,
            header::Header,
        };
        use cosmwasm_std::{
            coins,
            testing::{message_info, mock_env},
            Binary, Timestamp,
        };

        use crate::{
            contract::{
                instantiate, query, sudo,
                tests::{DUMMY_DATA, KEYS, SIGS},
            },
            msg::{
                InstantiateMsg, QueryMsg, SudoMsg, UpdateStateMsg, UpdateStateResult,
                VerifyClientMessageMsg,
            },
            test::helpers::mk_deps,
            ContractError,
        };
        #[test]
        fn basic_client_update_flow() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 100,
                is_frozen: false,
                min_required_sigs: 5,
            };
            let consensus_state = AttestorConsensusState {
                height: 100,
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

            // Create a header for client update (height progression)
            let header = Header {
                new_height: 101,
                timestamp: 1234567900, // 10 seconds later
                attestation_data: DUMMY_DATA.to_vec(),
                signatures: SIGS.to_vec(),
                pubkeys: KEYS.to_vec(),
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
                header.new_height,
                update_state_result.heights[0].revision_height
            );
        }

        #[test]
        fn incremental_client_update_flow() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 100,
                is_frozen: false,
                min_required_sigs: 5,
            };
            let consensus_state = AttestorConsensusState {
                height: 100,
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

            for i in 1..6 {
                // Create a header for client update (height progression)
                let header = Header {
                    new_height: consensus_state.height + i,
                    timestamp: consensus_state.timestamp + i,
                    attestation_data: DUMMY_DATA.to_vec(),
                    signatures: SIGS.to_vec(),
                    pubkeys: KEYS.to_vec(),
                };
                let header_bz = serde_json::to_vec(&header).unwrap();

                let mut env = mock_env();
                env.block.time = Timestamp::from_seconds(header.timestamp + 100);

                // Verify client message
                let query_verify_client_msg =
                    QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
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
                    header.new_height,
                    update_state_result.heights[0].revision_height
                );
            }
        }
        #[test]
        fn client_update_flow_with_historical_updates() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 100,
                is_frozen: false,
                min_required_sigs: 5,
            };
            let consensus_state = AttestorConsensusState {
                height: 100,
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

            // Add some even states
            for i in 1..6 {
                if i % 2 == 0 {
                    continue;
                }
                let header = Header {
                    new_height: consensus_state.height + i,
                    timestamp: consensus_state.timestamp + i,
                    attestation_data: DUMMY_DATA.to_vec(),
                    signatures: SIGS.to_vec(),
                    pubkeys: KEYS.to_vec(),
                };
                let header_bz = serde_json::to_vec(&header).unwrap();

                let mut env = mock_env();
                env.block.time = Timestamp::from_seconds(header.timestamp + 100);

                let query_verify_client_msg =
                    QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                        client_message: Binary::from(header_bz.clone()),
                    });
                query(deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();

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
                    header.new_height,
                    update_state_result.heights[0].revision_height
                );
            }

            // Retroactively add odd states
            for i in 1..6 {
                if i % 2 == 1 {
                    continue;
                }
                let header = Header {
                    new_height: consensus_state.height + i,
                    timestamp: consensus_state.timestamp + i,
                    attestation_data: DUMMY_DATA.to_vec(),
                    signatures: SIGS.to_vec(),
                    pubkeys: KEYS.to_vec(),
                };
                let header_bz = serde_json::to_vec(&header).unwrap();

                let mut env = mock_env();
                env.block.time = Timestamp::from_seconds(header.timestamp + 100);

                let query_verify_client_msg =
                    QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                        client_message: Binary::from(header_bz.clone()),
                    });
                query(deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();

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
                    header.new_height,
                    update_state_result.heights[0].revision_height
                );
            }
        }

        #[test]
        fn updates_fail_on_non_monotonic_client_updates() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 100,
                is_frozen: false,
                min_required_sigs: 5,
            };
            let consensus_state = AttestorConsensusState {
                height: 100,
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

            // Add some even states
            for i in 1..6 {
                if i % 2 == 0 {
                    continue;
                }
                let header = Header {
                    new_height: consensus_state.height + i,
                    timestamp: consensus_state.timestamp + i,
                    attestation_data: DUMMY_DATA.to_vec(),
                    signatures: SIGS.to_vec(),
                    pubkeys: KEYS.to_vec(),
                };
                let header_bz = serde_json::to_vec(&header).unwrap();

                let mut env = mock_env();
                env.block.time = Timestamp::from_seconds(header.timestamp + 100);

                let query_verify_client_msg =
                    QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                        client_message: Binary::from(header_bz.clone()),
                    });
                query(deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();

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
                    header.new_height,
                    update_state_result.heights[0].revision_height
                );
            }

            // Retroactively add odd states
            for i in 1..6 {
                if i % 2 == 1 {
                    continue;
                }

                let timestamp_with_same_time_as_previous = consensus_state.timestamp + i - 1;
                let header = Header {
                    new_height: consensus_state.height + i,
                    timestamp: timestamp_with_same_time_as_previous,
                    attestation_data: DUMMY_DATA.to_vec(),
                    signatures: SIGS.to_vec(),
                    pubkeys: KEYS.to_vec(),
                };
                let header_bz = serde_json::to_vec(&header).unwrap();

                let mut env = mock_env();
                env.block.time = Timestamp::from_seconds(header.timestamp + 100);

                let query_verify_client_msg =
                    QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                        client_message: Binary::from(header_bz.clone()),
                    });
                let res = query(deps.as_ref(), env.clone(), query_verify_client_msg);
                assert!(matches!(
                    res,
                    Err(ContractError::VerifyClientMessageFailed(
                        SolanaIBCError::InvalidHeader { reason }
                    ))
                        if reason.contains("timestamp")
                ));
            }
        }

        #[test]
        fn inconsistent_timestamp_for_existing_consensus_state() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 100,
                is_frozen: false,
                min_required_sigs: 5,
            };
            let consensus_state = AttestorConsensusState {
                height: 100,
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

            let header_with_different_ts_for_existing_height = Header {
                new_height: 100,
                timestamp: 12345654321,
                attestation_data: DUMMY_DATA.to_vec(),
                signatures: SIGS.to_vec(),
                pubkeys: KEYS.to_vec(),
            };
            let header_bz =
                serde_json::to_vec(&header_with_different_ts_for_existing_height).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(
                header_with_different_ts_for_existing_height.timestamp + 100,
            );

            // Verify client message should fail
            let query_verify_client_msg = QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                client_message: Binary::from(header_bz),
            });
            let err = query(deps.as_ref(), env, query_verify_client_msg).unwrap_err();
            assert!(matches!(
                err,
                ContractError::VerifyClientMessageFailed(
                    SolanaIBCError::InvalidHeader { reason }
                )
                    if reason.contains("timestamp")
            ));
        }

        #[test]
        fn bad_attestation_data() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 100,
                is_frozen: false,
                min_required_sigs: 5,
            };
            let consensus_state = AttestorConsensusState {
                height: 100,
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

            let header_with_random_data = Header {
                new_height: 100,
                timestamp: 1234567900,
                attestation_data: [156].to_vec(),
                signatures: SIGS.to_vec(),
                pubkeys: KEYS.to_vec(),
            };
            let header_bz = serde_json::to_vec(&header_with_random_data).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(header_with_random_data.timestamp + 100);

            // Verify client message should fail
            let query_verify_client_msg = QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                client_message: Binary::from(header_bz),
            });
            let err = query(deps.as_ref(), env, query_verify_client_msg).unwrap_err();
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
            let client_state = AttestorClientState {
                pub_keys: KEYS.clone(),
                latest_height: 100,
                is_frozen: true, // Client is frozen
                min_required_sigs: 5,
            };
            let consensus_state = AttestorConsensusState {
                height: 100,
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
                new_height: 100,
                timestamp: 1234567900,
                attestation_data: [].into(),
                signatures: SIGS.to_vec(),
                pubkeys: KEYS.to_vec(),
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
    }
}
