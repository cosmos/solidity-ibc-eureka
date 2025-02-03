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
use sp1_ics07_tendermint_membership::membership;

use ibc_core_commitment_types::merkle::MerkleProof;

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

    let request_iter = (0..request_len).map(|_| {
        // loop_encoded_1 is the path we want to verify the membership of
        let loop_encoded_1 = sp1_zkvm::io::read_vec();
        let path = bincode::deserialize(&loop_encoded_1).unwrap();

        // loop_encoded_2 is the value we want to prove the membership of
        // if it is empty, we are verifying non-membership
        let value = sp1_zkvm::io::read_vec();

        let loop_encoded_3 = sp1_zkvm::io::read_vec();
        let merkle_proof = MerkleProof::decode_vec(&loop_encoded_3).unwrap();

        (path, value, merkle_proof)
    });

    let output = membership(app_hash, request_iter);

    sp1_zkvm::io::commit_slice(&output.abi_encode());
}
