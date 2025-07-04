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
use ibc_client_tendermint::types::{ConsensusState, Header};
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::{ClientState as SolClientState, ConsensusState as SolConsensusState},
    IUpdateClientMsgs::UpdateClientOutput as SolUpdateClientOutput,
};
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use tendermint_light_client_update_client::{update_client, ClientState, TrustThreshold};
use ibc_core_client_types::{Height, timestamp::Timestamp};

/// Convert from Solidity ClientState to core ClientState
fn from_sol_client_state(cs: SolClientState) -> ClientState {
    ClientState {
        chain_id: cs.chainId,
        trust_level: TrustThreshold::new(
            cs.trustLevel.numerator.into(),
            cs.trustLevel.denominator.into(),
        ),
        trusting_period_seconds: cs.trustingPeriod.into(),
        unbonding_period_seconds: cs.unbondingPeriod.into(),
        max_clock_drift_seconds: cs.maxClockDrift.into(),
        frozen_height: if cs.frozenHeight.revisionHeight > 0 {
            Some(Height::new(cs.frozenHeight.revisionNumber, cs.frozenHeight.revisionHeight).expect("valid frozen height"))
        } else {
            None
        },
        latest_height: Height::new(cs.latestHeight.revisionNumber, cs.latestHeight.revisionHeight).expect("valid latest height"),
    }
}

/// Convert from Solidity ConsensusState to tendermint ConsensusState
fn from_sol_consensus_state(cs: SolConsensusState) -> ConsensusState {
    ConsensusState {
        root: cs.root.to_vec().try_into().expect("valid app hash"),
        next_validators_hash: cs.nextValidatorsHash.into(),
        timestamp: Timestamp::from_nanoseconds(cs.timestamp.try_into().unwrap())
            .expect("timestamp must be valid nanoseconds"),
    }
}

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
    let client_state = from_sol_client_state(sol_client_state.clone());
    // input 2: the trusted consensus state
    let sol_trusted_consensus_state = SolConsensusState::abi_decode(&encoded_2).unwrap();
    let trusted_consensus_state = from_sol_consensus_state(sol_trusted_consensus_state.clone());
    // input 3: the proposed header
    let proposed_header = <Header as Protobuf<RawHeader>>::decode_vec(&encoded_3).unwrap();
    // input 4: time
    let time = u128::from_le_bytes(encoded_4.try_into().unwrap());

    let output = update_client(client_state, trusted_consensus_state, proposed_header.clone(), time);

    // Convert output to Solidity format
    let sol_output = SolUpdateClientOutput {
        clientState: sol_client_state,
        trustedConsensusState: sol_trusted_consensus_state.into(),
        newConsensusState: SolConsensusState {
            timestamp: output.new_consensus_state.timestamp.nanoseconds().try_into().unwrap(),
            root: output.new_consensus_state.root.into_vec().try_into().expect("root must be 32 bytes"),
            nextValidatorsHash: output.new_consensus_state.next_validators_hash.as_bytes().try_into().expect("next validators hash must be 32 bytes"),
        }.into(),
        time,
        trustedHeight: output.trusted_height.into(),
        newHeight: output.new_client_state.latest_height.into(),
    };

    sp1_zkvm::io::commit_slice(&sol_output.abi_encode());
}