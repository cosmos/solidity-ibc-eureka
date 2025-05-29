pub mod v1_2_0 {
    use std::marker::PhantomData;

    use alloy_primitives::B256;
    use cosmwasm_std::{
        coins,
        testing::{
            message_info, mock_dependencies, mock_env, MockApi, MockQuerier,
            MockQuerierCustomHandlerResult, MockStorage,
        },
        Binary, OwnedDeps, SystemResult, Timestamp,
    };
    use ethereum_light_client::test_utils::{
        bls_verifier::{aggreagate, fast_aggregate_verify},
        fixtures::{self, StepsFixture},
    };
    use ethereum_light_client_v1_2_0::{
        header::Header,
        test_utils::fixtures::{InitialState as InitialStateV1_2, RelayerMessages},
    };
    use ethereum_types::consensus::bls::{BlsPublicKey, BlsSignature};
    use prost::Message;

    use cw_ics08_wasm_eth_v1_2_0::{
        contract as contract_v1_2, custom_query::EthereumCustomQuery, msg as msg_v1_2,
    };
    use ibc_proto::ibc::lightclients::wasm::v1::ClientMessage;

    use crate::{contract, msg, test::helpers::mk_deps};

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
                    .collect::<Vec<BlsPublicKey>>();
                let message = B256::try_from(message.as_slice()).unwrap();
                let signature = BlsSignature::try_from(signature.as_slice()).unwrap();

                fast_aggregate_verify(&public_keys, message, signature).unwrap();

                SystemResult::Ok(cosmwasm_std::ContractResult::Ok::<Binary>(
                    serde_json::to_vec(&true).unwrap().into(),
                ))
            }
            EthereumCustomQuery::Aggregate { public_keys } => {
                let public_keys = public_keys
                    .iter()
                    .map(|pk| pk.as_ref().try_into().unwrap())
                    .collect::<Vec<BlsPublicKey>>();

                let aggregate_pubkey = aggreagate(&public_keys).unwrap();

                SystemResult::Ok(cosmwasm_std::ContractResult::Ok::<Binary>(
                    serde_json::to_vec(&Binary::from(aggregate_pubkey.as_slice()))
                        .unwrap()
                        .into(),
                ))
            }
        }
    }

    pub fn mk_deps_v1_2(
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

    #[test]
    fn test_migrate_from_v1_2_0() {
        // Initialize v1_2_0
        let mut deps = mk_deps_v1_2();

        let creator = deps.api.addr_make("creator");
        let info = message_info(&creator, &coins(1, "uatom"));

        let fixture: StepsFixture =
            fixtures::load("Test_ICS20TransferNativeCosmosCoinsToEthereumAndBack");

        let initial_state: InitialStateV1_2 = fixture.get_data_at_step(0);

        let client_state = initial_state.client_state;

        let consensus_state = initial_state.consensus_state;

        let client_state_bz: Vec<u8> = serde_json::to_vec(&client_state).unwrap();
        let consensus_state_bz: Vec<u8> = serde_json::to_vec(&consensus_state).unwrap();

        let msg = msg_v1_2::InstantiateMsg {
            client_state: Binary::from(client_state_bz),
            consensus_state: Binary::from(consensus_state_bz),
            checksum: b"checksum".into(),
        };

        contract_v1_2::instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // Update the client state using v1_2
        let relayer_messages: RelayerMessages = fixture.get_data_at_step(1);
        let (update_client_msgs, _, _) = relayer_messages.get_sdk_msgs();
        assert_eq!(1, update_client_msgs.len()); // just to make sure
        let client_msgs = update_client_msgs
            .iter()
            .map(|msg| {
                ClientMessage::decode(msg.client_message.clone().unwrap().value.as_slice()).unwrap()
            })
            .map(|msg| msg.data)
            .collect::<Vec<_>>();

        let mut env = mock_env();

        for header_bz in client_msgs {
            let header: Header = serde_json::from_slice(&header_bz).unwrap();
            env.block.time = Timestamp::from_seconds(
                header.consensus_update.attested_header.execution.timestamp + 1000,
            );

            let query_verify_client_msg =
                msg_v1_2::QueryMsg::VerifyClientMessage(msg_v1_2::VerifyClientMessageMsg {
                    client_message: Binary::from(header_bz.clone()),
                });
            contract_v1_2::query(deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();

            // Update state
            let sudo_update_state_msg = msg_v1_2::SudoMsg::UpdateState(msg_v1_2::UpdateStateMsg {
                client_message: Binary::from(header_bz),
            });
            let update_res =
                contract_v1_2::sudo(deps.as_mut(), env.clone(), sudo_update_state_msg).unwrap();
            let update_state_result: msg_v1_2::UpdateStateResult =
                serde_json::from_slice(&update_res.data.unwrap())
                    .expect("update state result should be deserializable");
            assert_eq!(1, update_state_result.heights.len());
            assert_eq!(0, update_state_result.heights[0].revision_number);
            assert_eq!(
                header.consensus_update.finalized_header.beacon.slot,
                update_state_result.heights[0].revision_height
            );
        }

        let mut new_deps = mk_deps();
        new_deps.storage = deps.storage;

        // Migrate to current version
        // Migrate without any changes (i.e. same state version)
        contract::migrate(
            new_deps.as_mut(),
            mock_env(),
            msg::MigrateMsg {
                migration: msg::Migration::CodeOnly,
            },
        )
        .unwrap();

        // Check that the client can still be updated
        let relayer_messages: RelayerMessages = fixture.get_data_at_step(2);
        let (update_client_msgs, _, _) = relayer_messages.get_sdk_msgs();
        assert_eq!(1, update_client_msgs.len()); // just to make sure
        let client_msgs = update_client_msgs
            .iter()
            .map(|msg| {
                ClientMessage::decode(msg.client_message.clone().unwrap().value.as_slice()).unwrap()
            })
            .map(|msg| msg.data)
            .collect::<Vec<_>>();

        let mut env = mock_env();

        for header_bz in client_msgs {
            let header: Header = serde_json::from_slice(&header_bz).unwrap();
            env.block.time = Timestamp::from_seconds(
                header.consensus_update.attested_header.execution.timestamp + 1000,
            );

            let query_verify_client_msg =
                msg::QueryMsg::VerifyClientMessage(msg::VerifyClientMessageMsg {
                    client_message: Binary::from(header_bz.clone()),
                });
            contract::query(new_deps.as_ref(), env.clone(), query_verify_client_msg).unwrap();

            // Update state
            let sudo_update_state_msg = msg::SudoMsg::UpdateState(msg::UpdateStateMsg {
                client_message: Binary::from(header_bz),
            });
            let update_res =
                contract::sudo(new_deps.as_mut(), env.clone(), sudo_update_state_msg).unwrap();
            let update_state_result: msg::UpdateStateResult =
                serde_json::from_slice(&update_res.data.unwrap())
                    .expect("update state result should be deserializable");
            assert_eq!(1, update_state_result.heights.len());
            assert_eq!(0, update_state_result.heights[0].revision_number);
            assert_eq!(
                header.consensus_update.finalized_header.beacon.slot,
                update_state_result.heights[0].revision_height
            );
        }
    }
}
