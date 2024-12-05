use alloy_primitives::{keccak256, Bytes, Keccak256, U256};
use alloy_rlp::encode_fixed_size;
use alloy_trie::{proof::verify_proof, Nibbles};
use ethereum_utils::hex::to_hex;

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::EthereumIBCError,
    types::storage_proof::StorageProof,
};

pub fn verify_membership(
    trusted_consensus_state: ConsensusState,
    client_state: ClientState,
    proof: Vec<u8>,
    path: Vec<Vec<u8>>,
    raw_value: Option<Vec<u8>>,
) -> Result<(), EthereumIBCError> {
    let path = path.first().ok_or(EthereumIBCError::EmptyPath)?;

    let storage_proof: StorageProof = serde_json::from_slice(proof.as_slice())
        .map_err(|_| EthereumIBCError::StorageProofDecode)?;

    check_commitment_key(
        path.to_vec(),
        client_state.ibc_commitment_slot,
        storage_proof.key.into(),
    )?;

    let mut value = raw_value.clone();
    if let Some(raw_value) = raw_value {
        // membership proof (otherwise non-membership proof)
        let proof_value = storage_proof.value.to_be_bytes_vec();
        if proof_value != raw_value {
            return Err(EthereumIBCError::StoredValueMistmatch(
                to_hex(raw_value),
                to_hex(proof_value),
            ));
        }

        value = Some(encode_fixed_size(&U256::from_be_slice(&proof_value)).to_vec());
    }

    let proof: Vec<&Bytes> = storage_proof.proof.iter().map(|b| &b.0).collect();

    verify_proof::<Vec<&Bytes>>(
        trusted_consensus_state.storage_root,
        Nibbles::unpack(keccak256(storage_proof.key)),
        value,
        proof,
    )
    .map_err(|err| EthereumIBCError::VerifyStorageProof(err.to_string()))
}

fn check_commitment_key(
    path: Vec<u8>,
    ibc_commitment_slot: U256,
    key: U256,
) -> Result<(), EthereumIBCError> {
    let expected_commitment_key = ibc_commitment_key_v2(path, ibc_commitment_slot);

    // Data MUST be stored to the commitment path that is defined in ICS23.
    if expected_commitment_key != key {
        Err(EthereumIBCError::InvalidCommitmentKey(
            format!("0x{expected_commitment_key:x}"),
            format!("0x{key:x}"),
        ))
    } else {
        Ok(())
    }
}

// TODO: Unit test
#[must_use = "calculating the commitment key has no effect"]
fn ibc_commitment_key_v2(path: Vec<u8>, slot: U256) -> U256 {
    let path_hash = keccak256(path);

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
        test::fixtures::{load_fixture, CommitmentProofFixture},
        types::{storage_proof::StorageProof, wrappers::WrappedBytes},
    };
    use alloy_primitives::{
        hex::{self, FromHex},
        Bytes, B256, U256,
    };
    use ethereum_utils::hex::FromBeHex;

    use super::verify_membership;

    #[test]
    fn test_with_fixture() {
        let commitment_proof_fixture: CommitmentProofFixture = load_fixture(
            "TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Groth16_4_commitment_proof",
        );

        let trusted_consensus_state = commitment_proof_fixture.consensus_state;
        let client_state = commitment_proof_fixture.client_state;
        let storage_proof = commitment_proof_fixture.storage_proof;
        let path = commitment_proof_fixture.path;
        let value = storage_proof.value.to_be_bytes_vec();

        verify_membership(
            trusted_consensus_state,
            client_state,
            serde_json::to_vec(&storage_proof).unwrap(),
            vec![path],
            Some(value),
        )
        .unwrap();
    }

    #[test]
    fn test_verify_membership() {
        let client_state: ClientState = ClientState {
            ibc_commitment_slot: U256::from_be_hex(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            ),
            ..Default::default()
        };

        let consensus_state: ConsensusState = ConsensusState {
            storage_root: B256::from_hex(
                "0xe488caae2c0464e311e4a2df82bc74885fa81778d04131db6af3a451110a5eb5",
            )
            .unwrap(),
            ..Default::default()
        };

        let key =
            B256::from_hex("0x75d7411cb01daad167713b5a9b7219670f0e500653cbbcd45cfe1bfe04222459")
                .unwrap();
        let value =
            U256::from_be_hex("0xb2ae8ab0be3bda2f81dc166497902a1832fea11b886bc7a0980dec7a219582db");

        let proof = vec![
            WrappedBytes(Bytes::from_hex("0xf8718080a0911797c4b8cdbd1d8fa643b31ff0a469fae0f9b2ecbb0fa45a5ebe497f5e7130a065ea7eb6ae4e9747a131961beda4e9fd3040521e58845f4a286fb472eb0415168080a057b16d9a3bbb2d106b4d1b12dca3504f61899c7c660b036848511426ed342dd680808080808080808080").unwrap()),
            WrappedBytes(Bytes::from_hex("0xf843a03d3c3bcf030006afea2a677a6ff5bf3f7f111e87461c8848cf062a5756d1a888a1a0b2ae8ab0be3bda2f81dc166497902a1832fea11b886bc7a0980dec7a219582db").unwrap()),
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
            Some(value.to_be_bytes_vec()),
        )
        .unwrap();

        // should fail as a non-membership proof
        let value = U256::from(0);
        let storage_proof = StorageProof { key, value, proof };
        let storage_proof_bz = serde_json::to_vec(&storage_proof).unwrap();

        verify_membership(consensus_state, client_state, storage_proof_bz, path, None).unwrap_err();
    }

    #[test]
    fn test_verify_non_membership() {
        let client_state: ClientState = ClientState {
            ibc_commitment_slot: U256::from_be_hex(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            ),
            ..Default::default()
        };

        let consensus_state: ConsensusState = ConsensusState {
            storage_root: B256::from_hex(
                "0x8fce1302ff9ebea6343badec86e9814151872067d2dd47de08ec83e9bc7d22b3",
            )
            .unwrap(),
            ..Default::default()
        };

        let key =
            B256::from_hex("0x7a0c5ed5d5cb00ab03f4363e63deb3b05017026890db9f2110e931630567bf93")
                .unwrap();

        let proof = vec![
            WrappedBytes(Bytes::from_hex("0xf838a120290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e5639594eb9407e2a087056b69d43d21df69b82e31533c8a").unwrap()),
        ];

        let path = vec![hex::decode("0x30372d74656e6465726d696e742d30020000000000000001").unwrap()];

        let value = U256::from(0);
        let proof = StorageProof { key, value, proof };
        let proof_bz = serde_json::to_vec(&proof).unwrap();

        verify_membership(
            consensus_state.clone(),
            client_state.clone(),
            proof_bz.clone(),
            path.clone(),
            None,
        )
        .unwrap();

        // should fail as a membership proof
        verify_membership(
            consensus_state,
            client_state,
            proof_bz,
            path,
            Some(value.to_be_bytes_vec()),
        )
        .unwrap_err();
    }
}
