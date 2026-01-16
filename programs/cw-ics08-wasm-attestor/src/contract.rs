//! This module contains the `CosmWasm` entrypoints for the 08-wasm smart contract

use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, SudoMsg};
use crate::{instantiate, query};
use crate::{sudo, ContractError};

/// The version of the contracts state.
/// It is used to determine if the state needs to be migrated in the migrate entry point.
const STATE_VERSION: &str = env!("CARGO_PKG_VERSION");
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");

/// The instantiate entry point for the `CosmWasm` contract.
/// The instantiate entry point for the `CosmWasm` contract.
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

/// The sudo entry point for the `CosmWasm` contract.
/// It routes the message to the appropriate handler.
/// # Errors
/// Will return an error if the handler returns an error.
#[entry_point]
#[allow(clippy::needless_pass_by_value)]
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let result = match msg {
        SudoMsg::UpdateState(update_state_msg) => sudo::update_state(deps, update_state_msg)?,
        SudoMsg::VerifyMembership(verify_membership_msg) => {
            sudo::verify_membership(deps.as_ref(), verify_membership_msg)?
        }
        SudoMsg::VerifyNonMembership(verify_non_membership_msg) => {
            sudo::verify_non_membership(deps.as_ref(), verify_non_membership_msg)?
        }
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

/// The query entry point for the `CosmWasm` contract.
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
        QueryMsg::TimestampAtHeight(msg) => query::timestamp_at_height(deps, msg),
        QueryMsg::Status(_) => query::status(deps),
    }
}

#[cfg(test)]
mod tests {
    use attestor_light_client::{
        client_state::ClientState,
        consensus_state::ConsensusState,
        header::Header,
        test_utils::{
            packet_commitments_with_height, sample_packet_commitments, sigs_with_height, KEYS,
            MEMBERSHIP_PATH,
        },
    };
    use cosmwasm_std::Binary;

    use alloy_sol_types::SolValue;

    use crate::msg::InstantiateMsg;

    pub fn membership_value() -> Binary {
        sample_packet_commitments()[0].commitment.to_vec().into()
    }

    pub fn consensus() -> ConsensusState {
        ConsensusState {
            height: 42,
            timestamp: 1_234_567_890,
        }
    }

    pub fn client_state() -> ClientState {
        ClientState {
            attestor_addresses: KEYS.clone(),
            latest_height: 42,
            is_frozen: false,
            min_required_sigs: 5,
        }
    }

    pub fn header(cns: &ConsensusState) -> Header {
        Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: packet_commitments_with_height(cns.height).abi_encode(),
            signatures: sigs_with_height(cns.height),
        }
    }

    pub fn make_instatiate_msg(cs: &ClientState, cns: &ConsensusState) -> InstantiateMsg {
        let client_state_bz: Vec<u8> = serde_json::to_vec(&cs).unwrap();
        let consensus_state_bz: Vec<u8> = serde_json::to_vec(&cns).unwrap();

        InstantiateMsg {
            client_state: client_state_bz.into(),
            consensus_state: consensus_state_bz.into(),
            checksum: b"solana_checksum".into(),
        }
    }

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

        use crate::{
            contract::{
                instantiate,
                tests::{client_state, consensus, make_instatiate_msg},
            },
            state::{consensus_db_key, HOST_CLIENT_STATE_KEY},
            test::helpers::mk_deps,
        };

        #[test]
        fn assigns_correct_values() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

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
        use alloy_sol_types::SolValue;
        use attestor_light_client::{error::IbcAttestorClientError, membership::MembershipProof};
        use cosmwasm_std::{
            coins,
            testing::{message_info, mock_env},
            Binary, Timestamp,
        };
        use ibc_eureka_solidity_types::msgs::IAttestationMsgs::{PacketAttestation, PacketCompact};

        use crate::{
            contract::{
                instantiate, query, sudo,
                tests::{client_state, consensus, header, make_instatiate_msg, membership_value},
            },
            msg::{
                Height, MerklePath, QueryMsg, SudoMsg, TimestampAtHeightMsg,
                TimestampAtHeightResult, UpdateStateMsg, UpdateStateResult, VerifyClientMessageMsg,
                VerifyMembershipMsg,
            },
            test::helpers::mk_deps,
            ContractError,
        };

        use super::*;

        #[test]
        fn query_timestamp_at_height() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            let msg = QueryMsg::TimestampAtHeight(TimestampAtHeightMsg {
                height: Height {
                    revision_number: 0,
                    revision_height: consensus_state.height,
                },
            });
            let result = query(deps.as_ref(), mock_env(), msg);
            assert!(result.is_ok());
            let ts: TimestampAtHeightResult =
                serde_json::from_slice(&result.unwrap()).expect("must deserialize");
            assert_eq!(ts.timestamp, consensus_state.timestamp * 1_000_000_000);

            let non_existant_height = QueryMsg::TimestampAtHeight(TimestampAtHeightMsg {
                height: Height {
                    revision_number: 0,
                    revision_height: consensus_state.height + 1,
                },
            });
            let result = query(deps.as_ref(), mock_env(), non_existant_height);
            assert!(matches!(result, Err(ContractError::ConsensusStateNotFound)));
        }

        #[test]
        #[allow(clippy::too_many_lines)]
        fn basic_client_update_flow() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            let header = header(&consensus_state);
            let header_bz = serde_json::to_vec(&header).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(header.timestamp + 100);

            // Verify and Update state
            let query_verify_client_msg = QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                client_message: Binary::from(header_bz.clone()),
            });
            query(deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();
            let sudo_update_state_msg = SudoMsg::UpdateState(UpdateStateMsg {
                client_message: Binary::from(header_bz),
            });
            let update_res = sudo(deps.as_mut(), env, sudo_update_state_msg).unwrap();
            let update_state_result: UpdateStateResult =
                serde_json::from_slice(&update_res.data.unwrap())
                    .expect("update state result should be deserializable");

            assert_eq!(1, update_state_result.heights.len());
            assert_eq!(0, update_state_result.heights[0].revision_number);
            assert_eq!(
                header.new_height,
                update_state_result.heights[0].revision_height
            );

            let height = consensus_state.height;
            // Verify membership for the added state
            let env = mock_env();
            let verifyable = MembershipProof {
                attestation_data: packet_commitments_with_height(height).abi_encode(),
                signatures: sigs_with_height(height),
            };
            let as_bytes = serde_json::to_vec(&verifyable).unwrap();
            let msg = SudoMsg::VerifyMembership(VerifyMembershipMsg {
                height: Height {
                    revision_number: 0,
                    revision_height: consensus_state.height,
                },
                delay_time_period: 0,
                delay_block_period: 0,
                proof: as_bytes.into(),
                merkle_path: MerklePath {
                    key_path: vec![MEMBERSHIP_PATH.to_vec().into()],
                },
                value: membership_value(),
            });
            let res = sudo(deps.as_mut(), env.clone(), msg);
            assert!(res.is_ok());

            // Verify membership fails for non-existant packet
            let as_bytes = serde_json::to_vec(&verifyable).unwrap();
            let missing_packet = serde_json::to_vec(b"this does not exist").unwrap();
            let msg = SudoMsg::VerifyMembership(VerifyMembershipMsg {
                height: Height {
                    revision_number: 0,
                    revision_height: consensus_state.height,
                },
                delay_time_period: 0,
                delay_block_period: 0,
                proof: as_bytes.into(),
                merkle_path: MerklePath {
                    key_path: vec![MEMBERSHIP_PATH.to_vec().into()],
                },
                value: missing_packet.into(),
            });
            let res = sudo(deps.as_mut(), env, msg);
            assert!(matches!(res,
                    Err(ContractError::VerifyMembershipFailed(IbcAttestorClientError::InvalidProof { reason }))
                    if reason.contains("commitment mismatch")));

            // Non existent height fails
            let env = mock_env();
            let value = MembershipProof {
                attestation_data: packet_commitments_with_height(height).abi_encode(),
                signatures: sigs_with_height(height),
            };
            let as_bytes = serde_json::to_vec(&value).unwrap();
            let bad_height = consensus_state.height + 100;
            let msg = SudoMsg::VerifyMembership(VerifyMembershipMsg {
                height: Height {
                    revision_number: 0,
                    revision_height: bad_height,
                },
                delay_time_period: 0,
                delay_block_period: 0,
                proof: as_bytes.into(),
                merkle_path: MerklePath {
                    key_path: vec![MEMBERSHIP_PATH.to_vec().into()],
                },
                value: membership_value(),
            });
            let res = sudo(deps.as_mut(), env, msg);
            assert!(matches!(res, Err(ContractError::ConsensusStateNotFound)));

            // Bad attestation fails
            let env = mock_env();
            let bad_commitments = vec![PacketCompact {
                path: [254u8; 32].into(),
                commitment: [254u8; 32].into(),
            }];

            let value = PacketAttestation {
                packets: bad_commitments,
                height,
            };
            let verifyable = MembershipProof {
                attestation_data: value.abi_encode(),
                signatures: sigs_with_height(height),
            };
            let as_bytes = serde_json::to_vec(&verifyable).unwrap();
            let msg = SudoMsg::VerifyMembership(VerifyMembershipMsg {
                height: Height {
                    revision_number: 0,
                    revision_height: consensus_state.height,
                },
                delay_time_period: 0,
                delay_block_period: 0,
                proof: as_bytes.into(),
                merkle_path: MerklePath {
                    key_path: vec![MEMBERSHIP_PATH.to_vec().into()],
                },
                value: membership_value(),
            });
            let res = sudo(deps.as_mut(), env, msg);
            assert!(matches!(
                res,
                Err(ContractError::VerifyMembershipFailed(
                    IbcAttestorClientError::UnknownAddressRecovered { .. }
                ))
            ));
        }

        #[test]
        fn incremental_client_update_flow() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            for i in 1..6 {
                let mut header = header(&consensus_state);
                header.new_height += i;
                header.timestamp += i;

                let header_bz = serde_json::to_vec(&header).unwrap();

                let mut env = mock_env();
                env.block.time = Timestamp::from_seconds(header.timestamp + 100);

                // Verify and update
                let query_verify_client_msg =
                    QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                        client_message: Binary::from(header_bz.clone()),
                    });
                query(deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();
                let sudo_update_state_msg = SudoMsg::UpdateState(UpdateStateMsg {
                    client_message: Binary::from(header_bz),
                });
                let update_res = sudo(deps.as_mut(), env, sudo_update_state_msg).unwrap();
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
        #[allow(clippy::too_many_lines)]
        fn client_update_flow_with_historical_updates() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Add some even states
            for i in 1..6 {
                if i % 2 == 0 {
                    continue;
                }
                let mut header = header(&consensus_state);
                header.new_height += i;
                header.timestamp += i;

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
                let update_res = sudo(deps.as_mut(), env, sudo_update_state_msg).unwrap();
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
                let mut header = header(&consensus_state);
                header.new_height += i;
                header.timestamp += i;

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
                let update_res = sudo(deps.as_mut(), env, sudo_update_state_msg).unwrap();
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

            // Now validate messages for all those states
            for i in 1..6 {
                let env = mock_env();

                let height = consensus_state.height + i;
                let value = MembershipProof {
                    attestation_data: packet_commitments_with_height(height).abi_encode(),
                    signatures: sigs_with_height(height),
                };
                let as_bytes = serde_json::to_vec(&value).unwrap();
                let msg = SudoMsg::VerifyMembership(VerifyMembershipMsg {
                    height: Height {
                        revision_number: 0,
                        revision_height: consensus_state.height + i,
                    },
                    delay_time_period: 0,
                    delay_block_period: 0,
                    proof: as_bytes.into(),
                    merkle_path: MerklePath {
                        key_path: vec![MEMBERSHIP_PATH.to_vec().into()],
                    },
                    value: membership_value(),
                });
                let res = sudo(deps.as_mut(), env.clone(), msg);
                assert!(res.is_ok());

                let value = MembershipProof {
                    attestation_data: packet_commitments_with_height(height).abi_encode(),
                    signatures: sigs_with_height(height),
                };
                let as_bytes = serde_json::to_vec(&value).unwrap();
                let msg = SudoMsg::VerifyMembership(VerifyMembershipMsg {
                    height: Height {
                        revision_number: 0,
                        revision_height: consensus_state.height + i,
                    },
                    delay_time_period: 0,
                    delay_block_period: 0,
                    proof: as_bytes.into(),
                    merkle_path: MerklePath {
                        key_path: vec![MEMBERSHIP_PATH.to_vec().into()],
                    },
                    value: membership_value(),
                });
                let res = sudo(deps.as_mut(), env.clone(), msg);
                assert!(res.is_ok());
            }
        }

        #[test]
        fn updates_fail_on_non_monotonic_client_updates() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup initial client state
            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Add some even states
            for i in 1..6 {
                if i % 2 == 0 {
                    continue;
                }
                let mut header = header(&consensus_state);
                header.new_height += i;
                header.timestamp += i;

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

                let mut header = header(&consensus_state);
                header.new_height += i;

                let timestamp_with_same_time_as_previous = consensus_state.timestamp + i - 1;
                header.timestamp = timestamp_with_same_time_as_previous;

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
                        IbcAttestorClientError::InvalidHeader { reason }
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
            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            let mut header_with_different_ts_for_existing_height = header(&consensus_state);
            header_with_different_ts_for_existing_height.timestamp += 3;

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
                    IbcAttestorClientError::InvalidHeader { reason }
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
            let client_state = client_state();
            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            let mut header_with_random_data = header(&consensus_state);
            header_with_random_data.attestation_data = [156].to_vec();

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
                ContractError::VerifyClientMessageFailed(
                    IbcAttestorClientError::UnknownAddressRecovered { .. }
                )
            ));
        }

        #[test]
        fn frozen_client() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            // Setup frozen client state
            let mut client_state = client_state();
            client_state.is_frozen = true;

            let consensus_state = consensus();
            let msg = make_instatiate_msg(&client_state, &consensus_state);

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            // Create a valid header
            let header = header(&consensus_state);
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
                ContractError::VerifyClientMessageFailed(IbcAttestorClientError::ClientFrozen)
            ));
        }
    }
}
