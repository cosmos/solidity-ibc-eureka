//! A program that verifies the membership or non-membership of a value in a commitment root.

#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings)]
#![allow(clippy::no_mangle_with_rust_abi)]
// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolValue;
use ibc_proto::Protobuf;

use ibc_eureka_solidity_types::msgs::IMembershipMsgs::{KVPair as SolKVPair, MembershipOutput as SolMembershipOutput};
use tendermint_light_client_membership::{KVPair, MembershipOutput};

use ibc_core_commitment_types::merkle::MerkleProof;

/// Convert from Solidity KVPair to core KVPair
fn from_sol_kvpair(kv: SolKVPair) -> KVPair {
    KVPair::new(kv.path.to_vec(), kv.value.to_vec())
}

/// Convert from core MembershipOutput to Solidity MembershipOutput
fn to_sol_output(output: MembershipOutput) -> SolMembershipOutput {
    SolMembershipOutput {
        commitmentRoot: output.commitment_root.into(),
        kvPairs: output.verified_kv_pairs.into_iter()
            .map(|kv| SolKVPair {
                path: kv.path.into(),
                value: kv.value.into(),
            })
            .collect(),
    }
}

/// The main function of the program.
///
/// # Panics
/// Panics if the verification fails.
pub fn main() {
    let encoded_1 = sp1_zkvm::io::read_vec();
    let app_hash: [u8; 32] = encoded_1.try_into().unwrap();

    // encoded_2 is the number of key-value pairs we want to verify
    let encoded_2 = sp1_zkvm::io::read_vec();
    let request_len = u16::from_le_bytes(encoded_2.try_into().unwrap());
    assert!(request_len != 0);

    let requests: Vec<_> = (0..request_len).map(|_| {
        // loop_encoded_1 is the key-value pair we want to verify the membership of
        let loop_encoded_1 = sp1_zkvm::io::read_vec();
        let sol_kv_pair = SolKVPair::abi_decode(&loop_encoded_1).unwrap();
        let kv_pair = from_sol_kvpair(sol_kv_pair);

        // loop_encoded_2 is the Merkle proof of the key-value pair
        let loop_encoded_2 = sp1_zkvm::io::read_vec();
        let merkle_proof = MerkleProof::decode_vec(&loop_encoded_2).unwrap();

        (kv_pair, merkle_proof)
    }).collect();

    let output = tendermint_light_client_membership::verify_membership(app_hash, requests);
    let sol_output = to_sol_output(output);

    sp1_zkvm::io::commit_slice(&sol_output.abi_encode());
}