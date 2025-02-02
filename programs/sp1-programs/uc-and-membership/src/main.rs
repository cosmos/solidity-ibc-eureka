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

use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use sp1_ics07_tendermint_uc_and_membership::update_client_and_membership;

use ibc_core_commitment_types::merkle::MerkleProof;

use ibc_client_tendermint_types::Header;
use ibc_eureka_solidity_types::msgs::IICS07TendermintMsgs::{
    ClientState as SolClientState, ConsensusState as SolConsensusState,
};

/// The main function of the program.
///
/// # Panics
/// Panics if the verification fails.
pub fn main() {
    let encoded_1 = sp1_zkvm::io::read_vec();
    let encoded_2 = sp1_zkvm::io::read_vec();
    let encoded_3 = sp1_zkvm::io::read_vec();
    let encoded_4 = sp1_zkvm::io::read_vec();
    // encoded_5 is the number of key-value pairs we want to verify
    let request_len = sp1_zkvm::io::read_vec()[0];
    assert!(request_len != 0);

    // input 1: the client state
    let client_state = SolClientState::abi_decode(&encoded_1, true).unwrap();
    // input 2: the trusted consensus state
    let trusted_consensus_state = SolConsensusState::abi_decode(&encoded_2, true)
        .unwrap()
        .into();
    // input 3: the proposed header
    let proposed_header = <Header as Protobuf<RawHeader>>::decode_vec(&encoded_3).unwrap();
    // input 4: time
    let time = u64::from_le_bytes(encoded_4.try_into().unwrap());
    // TODO: find an encoding that works for all the structs above. (#132)

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

    let output = update_client_and_membership(
        client_state,
        trusted_consensus_state,
        proposed_header,
        time,
        request_iter,
    );

    sp1_zkvm::io::commit_slice(&output.abi_encode());
}
