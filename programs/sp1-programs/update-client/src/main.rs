//! A program that verifies the next block header of a blockchain using an IBC tendermint light
//! client.

#![deny(missing_docs)]
#![deny(clippy::nursery, clippy::pedantic, warnings)]
#![allow(clippy::no_mangle_with_rust_abi)]
// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolValue;
use ibc_client_tendermint::types::Header;
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::{ClientState as SolClientState, ConsensusState as SolConsensusState},
    IUpdateClientMsgs::UpdateClientOutput as SolUpdateClientOutput,
};
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use sp1_ics07_utils::{
    to_sol_consensus_state, to_sol_height, to_tendermint_client_state,
    to_tendermint_consensus_state,
};
use tendermint_light_client_update_client::update_client;

/// The main function of the program.
///
/// # Panics
/// Panics if the verification fails.
pub fn main() {
    let encoded_1 = sp1_zkvm::io::read_vec();
    let encoded_2 = sp1_zkvm::io::read_vec();
    let encoded_3 = sp1_zkvm::io::read_vec();
    let encoded_4 = sp1_zkvm::io::read_vec();

    // input 1: the client state
    let sol_client_state = SolClientState::abi_decode(&encoded_1).unwrap();
    let client_state = to_tendermint_client_state(&sol_client_state);
    // input 2: the trusted consensus state
    let sol_consensus_state = SolConsensusState::abi_decode(&encoded_2).unwrap();
    let trusted_consensus_state = to_tendermint_consensus_state(&sol_consensus_state);
    // input 3: the proposed header
    let proposed_header = <Header as Protobuf<RawHeader>>::decode_vec(&encoded_3).unwrap();
    // input 4: time
    let time = u128::from_le_bytes(encoded_4.try_into().unwrap());

    let output = update_client(
        &client_state,
        &trusted_consensus_state,
        proposed_header,
        time,
    )
    .unwrap();

    // Convert output to Solidity format
    let sol_output = SolUpdateClientOutput {
        clientState: sol_client_state,
        trustedConsensusState: sol_consensus_state,
        newConsensusState: to_sol_consensus_state(output.new_consensus_state),
        time,
        trustedHeight: to_sol_height(output.trusted_height),
        newHeight: to_sol_height(output.latest_height),
    };

    sp1_zkvm::io::commit_slice(&sol_output.abi_encode());
}
