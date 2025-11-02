//! This module provides [`verify_membership`] function to verify the membership of a key in the
//! storage trie.

use alloy_primitives::{keccak256, Keccak256, U256};
use ethereum_trie_db::trie_db::{verify_storage_exclusion_proof, verify_storage_inclusion_proof};
use ethereum_types::execution::storage_proof::StorageProof;

use crate::{client_state::ClientState, consensus_state::ConsensusState, error::EthereumIBCError};

/// Verifies the membership of a key in the storage trie.
/// # Errors
/// Returns an error if the proof cannot be verified.
#[allow(clippy::module_name_repetitions, clippy::needless_pass_by_value)]
pub fn verify_membership(
    trusted_consensus_state: ConsensusState,
    client_state: ClientState,
    proof: Vec<u8>,
    path: Vec<Vec<u8>>,
    raw_value: Vec<u8>,
) -> Result<(), EthereumIBCError> {
    let storage_proof: StorageProof = serde_json::from_slice(proof.as_slice())
        .map_err(|_| EthereumIBCError::StorageProofDecode)?;

    check_commitment_path(
        &path,
        client_state.ibc_commitment_slot,
        storage_proof.key.into(),
    )?;

    ensure!(
        storage_proof.value.to_be_bytes_vec() == raw_value,
        EthereumIBCError::StoredValueMistmatch {
            expected: raw_value,
            actual: storage_proof.value.to_be_bytes_vec(),
        }
    );

    let rlp_value = alloy_rlp::encode_fixed_size(&storage_proof.value);
    verify_storage_inclusion_proof(
        &trusted_consensus_state.storage_root,
        &storage_proof.key,
        &rlp_value,
        storage_proof.proof.iter(),
    )
    .map_err(|err| EthereumIBCError::VerifyStorageProof(err.to_string()))
}

/// Verifies the non-membership of a key in the storage trie.
/// # Errors
/// Returns an error if the proof cannot be verified.
#[allow(clippy::module_name_repetitions, clippy::needless_pass_by_value)]
pub fn verify_non_membership(
    trusted_consensus_state: ConsensusState,
    client_state: ClientState,
    proof: Vec<u8>,
    path: Vec<Vec<u8>>,
) -> Result<(), EthereumIBCError> {
    let storage_proof: StorageProof = serde_json::from_slice(proof.as_slice())
        .map_err(|_| EthereumIBCError::StorageProofDecode)?;

    check_commitment_path(
        &path,
        client_state.ibc_commitment_slot,
        storage_proof.key.into(),
    )?;

    ensure!(
        storage_proof.value.is_zero(),
        EthereumIBCError::StoredValueMistmatch {
            expected: vec![0],
            actual: storage_proof.value.to_be_bytes_vec(),
        }
    );

    verify_storage_exclusion_proof(
        &trusted_consensus_state.storage_root,
        &storage_proof.key,
        storage_proof.proof.iter(),
    )
    .map_err(|err| EthereumIBCError::VerifyStorageProof(err.to_string()))
}

fn check_commitment_path(
    path: &[Vec<u8>],
    ibc_commitment_slot: U256,
    key: U256,
) -> Result<(), EthereumIBCError> {
    ensure!(
        path.len() == 1,
        EthereumIBCError::InvalidPathLength {
            expected: 1,
            found: path.len()
        }
    );

    let expected_commitment_path = evm_ics26_commitment_path(&path[0], ibc_commitment_slot);
    ensure!(
        expected_commitment_path == key,
        EthereumIBCError::InvalidCommitmentKey(
            format!("0x{expected_commitment_path:x}"),
            format!("0x{key:x}"),
        )
    );

    Ok(())
}

// TODO: Unit test
/// Computes the commitment key for a given path and slot.
#[must_use = "calculating the commitment path has no effect"]
pub fn evm_ics26_commitment_path(ibc_path: &[u8], slot: U256) -> U256 {
    let path_hash = keccak256(ibc_path);

    let mut hasher = Keccak256::new();
    hasher.update(path_hash);
    hasher.update(slot.to_be_bytes_vec());

    hasher.finalize().into()
}

#[cfg(test)]
mod test {
    use crate::{
        client_state::ClientState,
        consensus_state::ConsensusState,
        header::Header,
        test_utils::fixtures::{self, get_packet_proof, InitialState, RelayerMessages},
        update::update_consensus_state,
    };

    use alloy_primitives::{
        hex::{self, FromHex},
        Bytes, FixedBytes, B256, U256,
    };
    use ethereum_types::{
        consensus::sync_committee::SummarizedSyncCommittee, execution::storage_proof::StorageProof,
    };
    use ibc_proto_eureka::ibc::lightclients::wasm::v1::ClientMessage;

    use prost::Message;

    use super::{verify_membership, verify_non_membership};

    #[test]
    fn test_with_fixture() {
        let fixture: fixtures::StepsFixture =
            fixtures::load("Test_ICS20TransferERC20TokenfromEthereumToCosmosAndBack");

        let initial_state: InitialState = fixture.get_data_at_step(0);

        let relayer_messages: RelayerMessages = fixture.get_data_at_step(1);
        let (update_client_msgs, recv_msgs, _) = relayer_messages.get_sdk_msgs();
        assert!(!update_client_msgs.is_empty());
        assert!(!recv_msgs.is_empty());

        let headers = update_client_msgs
            .iter()
            .map(|msg| {
                let client_msg =
                    ClientMessage::decode(msg.client_message.clone().unwrap().value.as_slice())
                        .unwrap();
                serde_json::from_slice(client_msg.data.as_slice()).unwrap()
            })
            .collect::<Vec<Header>>();

        let mut latest_consensus_state = initial_state.consensus_state;
        let mut latest_client_state = initial_state.client_state;
        for header in headers {
            let (_, updated_consensus_state, updated_client_state) =
                update_consensus_state(latest_consensus_state, latest_client_state, header.clone())
                    .unwrap();

            latest_consensus_state = updated_consensus_state;
            latest_client_state = updated_client_state.unwrap();
        }

        let trusted_consensus_state = latest_consensus_state;
        let client_state = latest_client_state;

        let packet = recv_msgs[0].packet.clone().unwrap();
        let storage_proof = recv_msgs[0].proof_commitment.clone();
        let (path, value) = get_packet_proof(packet);

        verify_membership(
            trusted_consensus_state,
            client_state,
            storage_proof,
            vec![path],
            value,
        )
        .unwrap();
    }

    #[test]
    fn test_verify_membership() {
        let client_state: ClientState = ClientState {
            ibc_commitment_slot: from_be_hex(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            ),
            ..Default::default()
        };

        let consensus_state: ConsensusState = ConsensusState {
            slot: 0,
            storage_root: B256::from_hex(
                "0xe488caae2c0464e311e4a2df82bc74885fa81778d04131db6af3a451110a5eb5",
            )
            .unwrap(),
            state_root: FixedBytes::default(),
            timestamp: 0,
            current_sync_committee: SummarizedSyncCommittee::default(),
            next_sync_committee: None,
        };

        let key =
            B256::from_hex("0x75d7411cb01daad167713b5a9b7219670f0e500653cbbcd45cfe1bfe04222459")
                .unwrap();
        let value =
            from_be_hex("0xb2ae8ab0be3bda2f81dc166497902a1832fea11b886bc7a0980dec7a219582db");

        let proof = vec![
            Bytes::from_hex("0xf8718080a0911797c4b8cdbd1d8fa643b31ff0a469fae0f9b2ecbb0fa45a5ebe497f5e7130a065ea7eb6ae4e9747a131961beda4e9fd3040521e58845f4a286fb472eb0415168080a057b16d9a3bbb2d106b4d1b12dca3504f61899c7c660b036848511426ed342dd680808080808080808080").unwrap(),
            Bytes::from_hex("0xf843a03d3c3bcf030006afea2a677a6ff5bf3f7f111e87461c8848cf062a5756d1a888a1a0b2ae8ab0be3bda2f81dc166497902a1832fea11b886bc7a0980dec7a219582db").unwrap(),
        ];

        let path = vec![hex::decode("0x30372d74656e6465726d696e742d30010000000000000001").unwrap()];

        let storage_proof = StorageProof {
            key,
            value,
            proof: proof.clone(),
        };
        let storage_proof_bz = serde_json::to_vec(&storage_proof).unwrap();

        verify_membership(
            consensus_state.clone(),
            client_state.clone(),
            storage_proof_bz,
            path.clone(),
            value.to_be_bytes_vec(),
        )
        .unwrap();

        // should fail as a non-membership proof
        let value = U256::from(0);
        let storage_proof = StorageProof { key, value, proof };
        let storage_proof_bz = serde_json::to_vec(&storage_proof).unwrap();

        verify_non_membership(consensus_state, client_state, storage_proof_bz, path).unwrap_err();
    }

    #[test]
    fn test_verify_non_membership() {
        let client_state: ClientState = ClientState {
            ibc_commitment_slot: from_be_hex(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            ),
            ..Default::default()
        };

        let consensus_state: ConsensusState = ConsensusState {
            slot: 0,
            storage_root: B256::from_hex(
                "0x8fce1302ff9ebea6343badec86e9814151872067d2dd47de08ec83e9bc7d22b3",
            )
            .unwrap(),
            state_root: FixedBytes::default(),
            timestamp: 0,
            current_sync_committee: SummarizedSyncCommittee::default(),
            next_sync_committee: None,
        };

        let key =
            B256::from_hex("0x7a0c5ed5d5cb00ab03f4363e63deb3b05017026890db9f2110e931630567bf93")
                .unwrap();

        let proof = vec![
            Bytes::from_hex("0xf838a120290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e5639594eb9407e2a087056b69d43d21df69b82e31533c8a").unwrap(),
        ];

        let path = vec![hex::decode("0x30372d74656e6465726d696e742d30020000000000000001").unwrap()];

        let value = U256::from(0);
        let proof = StorageProof { key, value, proof };
        let proof_bz = serde_json::to_vec(&proof).unwrap();

        verify_non_membership(
            consensus_state.clone(),
            client_state.clone(),
            proof_bz.clone(),
            path.clone(),
        )
        .unwrap();

        // should fail as a membership proof
        verify_membership(
            consensus_state,
            client_state,
            proof_bz,
            path,
            value.to_be_bytes_vec(),
        )
        .unwrap_err();
    }

    fn from_be_hex(hex_str: &str) -> U256 {
        let data = hex::decode(hex_str).unwrap();
        U256::from_be_slice(data.as_slice())
    }
}
