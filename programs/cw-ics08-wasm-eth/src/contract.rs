use std::convert::Into;

use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response,
};
use ethereum_light_client::{
    client_state::ClientState as EthClientState,
    consensus_state::ConsensusState as EthConsensusState,
};
use ibc_proto::ibc::{
    core::client::v1::Height as IbcProtoHeight,
    lightclients::wasm::v1::{
        ClientState as WasmClientState, ConsensusState as WasmConsensusState,
    },
};
use prost::Message;
use tendermint_proto::google::protobuf::Any;

use crate::custom_query::{BlsVerifier, EthereumCustomQuery};
use crate::error::ContractError;
use crate::msg::{
    CheckForMisbehaviourResult, ExecuteMsg, ExportMetadataResult, Height, InstantiateMsg, QueryMsg,
    StatusResult, SudoMsg, TimestampAtHeightResult, UpdateStateResult, VerifyClientMessageMsg,
    VerifyMembershipMsg, VerifyNonMembershipMsg,
};
use crate::state::{
    consensus_db_key, get_eth_client_state, get_eth_consensus_state, HOST_CLIENT_STATE_KEY,
};

#[entry_point]
pub fn instantiate(
    deps: DepsMut<EthereumCustomQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let client_state_bz: Vec<u8> = msg.client_state.into();
    let client_state: EthClientState = serde_json::from_slice(&client_state_bz)
        .map_err(ContractError::DeserializeClientStateFailed)?;
    let wasm_client_state = WasmClientState {
        checksum: msg.checksum.into(),
        data: client_state_bz,
        latest_height: Some(IbcProtoHeight {
            revision_number: 0,
            revision_height: client_state.latest_slot,
        }),
    };
    let wasm_client_state_any = Any::from_msg(&wasm_client_state).unwrap();
    deps.storage.set(
        HOST_CLIENT_STATE_KEY.as_bytes(),
        wasm_client_state_any.encode_to_vec().as_slice(),
    );

    let consensus_state_bz: Vec<u8> = msg.consensus_state.into();
    let consensus_state: EthConsensusState = serde_json::from_slice(&consensus_state_bz)
        .map_err(ContractError::DeserializeClientStateFailed)?;
    let wasm_consensus_state = WasmConsensusState {
        data: consensus_state_bz,
    };
    let wasm_consensus_state_any = Any::from_msg(&wasm_consensus_state).unwrap();
    let height = Height {
        revision_number: 0,
        revision_height: consensus_state.slot,
    };
    deps.storage.set(
        consensus_db_key(&height).as_bytes(),
        wasm_consensus_state_any.encode_to_vec().as_slice(),
    );

    Ok(Response::default())
}

#[entry_point]
pub fn sudo(
    deps: DepsMut<EthereumCustomQuery>,
    _env: Env,
    msg: SudoMsg,
) -> Result<Response, ContractError> {
    let result = match msg {
        SudoMsg::VerifyMembership(verify_membership_msg) => {
            verify_membership(deps.as_ref(), verify_membership_msg)?
        }
        SudoMsg::VerifyNonMembership(verify_non_membership_msg) => {
            verify_non_membership(deps.as_ref(), verify_non_membership_msg)?
        }
        SudoMsg::UpdateState(_) => update_state()?,
        SudoMsg::UpdateStateOnMisbehaviour(_) => unimplemented!(),
        SudoMsg::VerifyUpgradeAndUpdateState(_) => unimplemented!(),
        SudoMsg::MigrateClientStore(_) => unimplemented!(),
    };

    Ok(Response::default().set_data(result))
}

pub fn verify_membership(
    deps: Deps<EthereumCustomQuery>,
    verify_membership_msg: VerifyMembershipMsg,
) -> Result<Binary, ContractError> {
    let eth_client_state = get_eth_client_state(deps);
    let eth_consensus_state = get_eth_consensus_state(deps, &verify_membership_msg.height);

    ethereum_light_client::membership::verify_membership(
        eth_consensus_state,
        eth_client_state,
        verify_membership_msg.proof.into(),
        verify_membership_msg
            .merkle_path
            .key_path
            .into_iter()
            .map(Into::into)
            .collect(),
        Some(verify_membership_msg.value.into()),
    )
    .map_err(ContractError::VerifyMembershipFailed)?;

    Ok(to_json_binary(&Ok::<(), ()>(()))?)
}

pub fn verify_non_membership(
    deps: Deps<EthereumCustomQuery>,
    verify_non_membership_msg: VerifyNonMembershipMsg,
) -> Result<Binary, ContractError> {
    let eth_client_state = get_eth_client_state(deps);
    let eth_consensus_state = get_eth_consensus_state(deps, &verify_non_membership_msg.height);

    ethereum_light_client::membership::verify_membership(
        eth_consensus_state,
        eth_client_state,
        verify_non_membership_msg.proof.into(),
        verify_non_membership_msg
            .merkle_path
            .key_path
            .into_iter()
            .map(Into::into)
            .collect(),
        None,
    )
    .map_err(ContractError::VerifyNonMembershipFailed)?;

    Ok(to_json_binary(&Ok::<(), ()>(()))?)
}

pub fn update_state() -> Result<Binary, ContractError> {
    Ok(to_json_binary(&UpdateStateResult { heights: vec![] })?)
}

#[entry_point]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    unimplemented!()
}

#[entry_point]
pub fn query(
    deps: Deps<EthereumCustomQuery>,
    env: Env,
    msg: QueryMsg,
) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::VerifyClientMessage(verify_client_message_msg) => {
            verify_client_message(deps, env, verify_client_message_msg)
        }
        QueryMsg::CheckForMisbehaviour(_) => check_for_misbehaviour(),
        QueryMsg::TimestampAtHeight(_) => timestamp_at_height(env),
        QueryMsg::Status(_) => status(),
        QueryMsg::ExportMetadata(_) => export_metadata(),
    }
}

pub fn verify_client_message(
    deps: Deps<EthereumCustomQuery>,
    env: Env,
    verify_client_message_msg: VerifyClientMessageMsg,
) -> Result<Binary, ContractError> {
    let eth_client_state = get_eth_client_state(deps);
    let eth_consensus_state = get_eth_consensus_state(
        deps,
        &Height {
            revision_number: 0,
            revision_height: eth_client_state.latest_slot,
        },
    );
    let header = serde_json::from_slice(&verify_client_message_msg.client_message)
        .map_err(ContractError::DeserializeClientStateFailed)?;
    let bls_verifier = BlsVerifier { deps };

    ethereum_light_client::verify::verify_header(
        &eth_consensus_state,
        &eth_client_state,
        env.block.time.seconds(),
        &header,
        bls_verifier,
    )
    .map_err(ContractError::VerifyClientMessageFailed)?;

    Ok(to_json_binary(&Ok::<(), ()>(()))?)
}

pub fn check_for_misbehaviour() -> Result<Binary, ContractError> {
    Ok(to_json_binary(&CheckForMisbehaviourResult {
        found_misbehaviour: false,
    })?)
}

pub fn timestamp_at_height(env: Env) -> Result<Binary, ContractError> {
    let now = env.block.time.seconds();
    Ok(to_json_binary(&TimestampAtHeightResult { timestamp: now })?)
}

pub fn status() -> Result<Binary, ContractError> {
    Ok(to_json_binary(&StatusResult {
        status: "Active".to_string(),
    })?)
}

pub fn export_metadata() -> Result<Binary, ContractError> {
    Ok(to_json_binary(&ExportMetadataResult {
        genesis_metadata: vec![],
    })?)
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use alloy_primitives::B256;
    use cosmwasm_std::{
        testing::{
            mock_dependencies, MockApi, MockQuerier, MockQuerierCustomHandlerResult, MockStorage,
        },
        Binary, OwnedDeps, SystemResult,
    };
    use ethereum_light_client::{
        client_state::ClientState as EthClientState,
        consensus_state::ConsensusState as EthConsensusState,
        types::{
            bls::{BlsPublicKey, BlsSignature},
            storage_proof::StorageProof,
        },
    };
    use ethereum_test_utils::bls_verifier::{aggreagate, fast_aggregate_verify};
    use serde::{Deserialize, Serialize};

    use crate::custom_query::EthereumCustomQuery;

    mod instantiate_tests {
        use alloy_primitives::{aliases::B32, FixedBytes, B256, U256};
        use cosmwasm_std::{
            coins,
            testing::{message_info, mock_env},
            Storage,
        };
        use ethereum_light_client::{
            client_state::ClientState as EthClientState,
            consensus_state::ConsensusState as EthConsensusState,
            types::{fork::Fork, fork_parameters::ForkParameters, wrappers::WrappedVersion},
        };
        use ibc_proto::ibc::lightclients::wasm::v1::{
            ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        };
        use prost::{Message, Name};
        use tendermint_proto::google::protobuf::Any;

        use crate::{
            contract::{instantiate, tests::mk_deps},
            msg::{Height, InstantiateMsg},
            state::{consensus_db_key, HOST_CLIENT_STATE_KEY},
        };

        #[test]
        fn test_instantiate() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state = EthClientState {
                chain_id: 0,
                genesis_validators_root: B256::from([0; 32]),
                min_sync_committee_participants: 0,
                genesis_time: 0,
                fork_parameters: ForkParameters {
                    genesis_fork_version: WrappedVersion(B32::from([0; 4])),
                    genesis_slot: 0,
                    altair: Fork {
                        version: WrappedVersion(B32::from([0; 4])),
                        epoch: 0,
                    },
                    bellatrix: Fork {
                        version: WrappedVersion(B32::from([0; 4])),
                        epoch: 0,
                    },
                    capella: Fork {
                        version: WrappedVersion(B32::from([0; 4])),
                        epoch: 0,
                    },
                    deneb: Fork {
                        version: WrappedVersion(B32::from([0; 4])),
                        epoch: 0,
                    },
                },
                seconds_per_slot: 0,
                slots_per_epoch: 0,
                epochs_per_sync_committee_period: 0,
                latest_slot: 42,
                ibc_commitment_slot: U256::from(0),
                ibc_contract_address: Default::default(),
                frozen_height: ethereum_light_client::types::height::Height::default(),
            };
            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();

            let consensus_state = EthConsensusState {
                slot: 0,
                state_root: B256::from([0; 32]),
                storage_root: B256::from([0; 32]),
                timestamp: 0,
                current_sync_committee: FixedBytes::<48>::from([0; 48]),
                next_sync_committee: None,
            };
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = InstantiateMsg {
                client_state: client_state_bz.into(),
                consensus_state: consensus_state_bz.into(),
                checksum: "also does not matter yet".as_bytes().into(),
            };

            let res = instantiate(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();
            assert_eq!(0, res.messages.len());

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
            assert_eq!(msg.consensus_state, actual_consensus_state.data);
        }
    }

    mod sudo_tests {
        use cosmwasm_std::{
            coins,
            testing::{message_info, mock_env},
            Binary,
        };
        use ethereum_test_utils::fixtures;

        use crate::{
            contract::{
                instantiate, sudo,
                tests::{mk_deps, CommitmentProofFixture},
            },
            msg::{Height, MerklePath, SudoMsg, UpdateStateMsg, VerifyMembershipMsg},
        };

        #[test]
        fn test_verify_membership() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let commitment_proof_fixture: CommitmentProofFixture = fixtures::load(
                "TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Groth16_4_commitment_proof",
            );

            let client_state = commitment_proof_fixture.client_state;
            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state = commitment_proof_fixture.consensus_state;
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = crate::msg::InstantiateMsg {
                client_state: Binary::from(client_state_bz),
                consensus_state: Binary::from(consensus_state_bz),
                checksum: "checksum".as_bytes().into(), // TODO: Real checksum important?
            };
            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            let proof = commitment_proof_fixture.storage_proof;
            let proof_bz = serde_json::to_vec(&proof).unwrap();
            let path = commitment_proof_fixture.path;
            let value = proof.value;
            let value_bz = value.to_be_bytes_vec();

            let msg = SudoMsg::VerifyMembership(VerifyMembershipMsg {
                height: Height {
                    revision_number: 0,
                    revision_height: commitment_proof_fixture.proof_height.revision_height,
                },
                delay_time_period: 0,
                delay_block_period: 0,
                proof: Binary::from(proof_bz),
                merkle_path: MerklePath {
                    key_path: vec![Binary::from(path)],
                },
                value: Binary::from(value_bz),
            });
            let res = sudo(deps.as_mut(), mock_env(), msg).unwrap();
            assert_eq!(0, res.messages.len());
        }

        //#[test]
        //fn test_verify_non_membership() {
        //    let mut deps = mk_deps();
        //    let msg = SudoMsg::VerifyNonMembership(VerifyNonMembershipMsg {
        //        height: Height {
        //            revision_number: 0,
        //            revision_height: 1,
        //        },
        //        delay_time_period: 0,
        //        delay_block_period: 0,
        //        proof: Binary::default(),
        //        merkle_path: MerklePath { key_path: vec![] },
        //    });
        //    let res = sudo(deps.as_mut(), mock_env(), msg).unwrap();
        //    assert_eq!(0, res.messages.len());
        //}

        #[test]
        fn test_update_state() {
            let mut deps = mk_deps();
            let msg = SudoMsg::UpdateState(UpdateStateMsg {
                client_message: Binary::default(),
            });
            let res = sudo(deps.as_mut(), mock_env(), msg).unwrap();
            assert_eq!(0, res.messages.len());
        }
    }

    mod query_tests {
        use cosmwasm_std::{
            coins, from_json,
            testing::{message_info, mock_env},
            Binary, Timestamp,
        };
        use ethereum_light_client::{
            client_state::ClientState as EthClientState,
            consensus_state::ConsensusState as EthConsensusState, types::light_client::Header,
        };
        use ethereum_test_utils::fixtures::load;

        use crate::{
            contract::{instantiate, query, tests::mk_deps},
            msg::{
                CheckForMisbehaviourMsg, CheckForMisbehaviourResult, ExportMetadataMsg,
                ExportMetadataResult, Height, QueryMsg, StatusMsg, StatusResult,
                TimestampAtHeightMsg, TimestampAtHeightResult, VerifyClientMessageMsg,
            },
        };

        #[test]
        fn test_verify_client_message() {
            let mut deps = mk_deps();
            let creator = deps.api.addr_make("creator");
            let info = message_info(&creator, &coins(1, "uatom"));

            let client_state: EthClientState = load(
                "TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Groth16_1_initial_client_state",
            );

            let consensus_state: EthConsensusState = load(
                "TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Groth16_2_initial_consensus_state",
            );

            let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
            let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

            let msg = crate::msg::InstantiateMsg {
                client_state: Binary::from(client_state_bz),
                consensus_state: Binary::from(consensus_state_bz),
                checksum: "checksum".as_bytes().into(), // TODO: Real checksum important?
            };

            instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

            let header: Header = load(
                "TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Groth16_3_update_header_0",
            );
            let header_bz: Vec<u8> = serde_json::to_vec(&header).unwrap();

            let mut env = mock_env();
            env.block.time = Timestamp::from_seconds(
                header.consensus_update.attested_header.execution.timestamp + 1000,
            );

            query(
                deps.as_ref(),
                env,
                QueryMsg::VerifyClientMessage(VerifyClientMessageMsg {
                    client_message: Binary::from(header_bz),
                }),
            )
            .unwrap();
        }

        #[test]
        fn test_check_for_misbehaviour() {
            let deps = mk_deps();
            let res = query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::CheckForMisbehaviour(CheckForMisbehaviourMsg {
                    client_message: Binary::default(),
                }),
            )
            .unwrap();
            let misbehaviour_result: CheckForMisbehaviourResult = from_json(&res).unwrap();
            assert!(!misbehaviour_result.found_misbehaviour);
        }

        #[test]
        fn test_timestamp_at_height() {
            let deps = mk_deps();
            let res = query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TimestampAtHeight(TimestampAtHeightMsg {
                    height: Height {
                        revision_number: 0,
                        revision_height: 1,
                    },
                }),
            )
            .unwrap();
            let timestamp_at_height_result: TimestampAtHeightResult = from_json(&res).unwrap();
            assert_eq!(
                mock_env().block.time.seconds(),
                timestamp_at_height_result.timestamp
            );
        }

        #[test]
        fn test_status() {
            let deps = mk_deps();
            let res = query(deps.as_ref(), mock_env(), QueryMsg::Status(StatusMsg {})).unwrap();
            let status_response: StatusResult = from_json(&res).unwrap();
            assert_eq!("Active", status_response.status);
        }

        #[test]
        fn test_export_metadata() {
            let deps = mk_deps();
            let res = query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::ExportMetadata(ExportMetadataMsg {}),
            )
            .unwrap();
            let export_metadata_result: ExportMetadataResult = from_json(&res).unwrap();
            assert_eq!(0, export_metadata_result.genesis_metadata.len());
        }
    }

    #[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
    pub struct CommitmentProofFixture {
        #[serde(with = "ethereum_utils::base64")]
        pub path: Vec<u8>,
        pub storage_proof: StorageProof,
        pub proof_height: ethereum_light_client::types::height::Height,
        pub client_state: EthClientState,
        pub consensus_state: EthConsensusState,
    }

    // TODO: Find a way to reuse the test handling code that already exists in the
    // ethereum-light-client package
    pub fn custom_query_handler(query: &EthereumCustomQuery) -> MockQuerierCustomHandlerResult {
        match query {
            EthereumCustomQuery::AggregateVerify {
                public_keys,
                message,
                signature,
            } => {
                let public_keys = public_keys
                    .iter()
                    .map(|pk| pk.as_ref().try_into().unwrap())
                    .collect::<Vec<&BlsPublicKey>>();
                let message = B256::try_from(message.as_slice()).unwrap();
                let signature = BlsSignature::try_from(signature.as_slice()).unwrap();

                fast_aggregate_verify(public_keys, message, signature).unwrap();

                SystemResult::Ok(cosmwasm_std::ContractResult::Ok::<Binary>(
                    serde_json::to_vec(&true).unwrap().into(),
                ))
            }
            EthereumCustomQuery::Aggregate { public_keys } => {
                let public_keys = public_keys
                    .iter()
                    .map(|pk| pk.as_ref().try_into().unwrap())
                    .collect::<Vec<&BlsPublicKey>>();

                let aggregate_pubkey = aggreagate(public_keys).unwrap();

                SystemResult::Ok(cosmwasm_std::ContractResult::Ok::<Binary>(
                    serde_json::to_vec(&Binary::from(aggregate_pubkey.as_slice()))
                        .unwrap()
                        .into(),
                ))
            }
        }
    }

    fn mk_deps(
    ) -> OwnedDeps<MockStorage, MockApi, MockQuerier<EthereumCustomQuery>, EthereumCustomQuery>
    {
        let deps = mock_dependencies();

        OwnedDeps {
            storage: deps.storage,
            api: deps.api,
            querier: MockQuerier::<EthereumCustomQuery>::new(&[])
                .with_custom_handler(custom_query_handler),
            custom_query_type: PhantomData,
        }
    }
}
