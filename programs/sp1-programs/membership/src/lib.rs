//! The crate that contains the types and utilities for `sp1-ics07-tendermint-membership` program.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings)]

use ibc_eureka_solidity_types::sp1_ics07::IMembershipMsgs::{KVPair, MembershipOutput};

use ibc_core_commitment_types::{
    commitment::CommitmentRoot,
    merkle::{MerklePath, MerkleProof},
    proto::ics23::HostFunctionsManager,
    specs::ProofSpecs,
};

/// The main function of the program without the zkVM wrapper.
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn membership(
    app_hash: [u8; 32],
    request_iter: impl Iterator<Item = (Vec<Vec<u8>>, Vec<u8>, MerkleProof)>,
) -> MembershipOutput {
    let commitment_root = CommitmentRoot::from_bytes(&app_hash);

    let kv_pairs = request_iter
        .map(|(path, value, merkle_proof)| {
            let merkle_path = MerklePath {
                key_path: path.into_iter().map(Into::into).collect(),
            };

            if value.is_empty() {
                merkle_proof
                    .verify_non_membership::<HostFunctionsManager>(
                        &ProofSpecs::cosmos(),
                        commitment_root.clone().into(),
                        merkle_path.clone(),
                    )
                    .unwrap();
            } else {
                merkle_proof
                    .verify_membership::<HostFunctionsManager>(
                        &ProofSpecs::cosmos(),
                        commitment_root.clone().into(),
                        merkle_path.clone(),
                        value.clone(),
                        0,
                    )
                    .unwrap();
            }

            KVPair {
                path: merkle_path
                    .key_path
                    .into_iter()
                    .map(|v| v.into_vec().into())
                    .collect(),
                value: value.into(),
            }
        })
        .collect();

    MembershipOutput {
        commitmentRoot: app_hash.into(),
        kvPairs: kv_pairs,
    }
}
