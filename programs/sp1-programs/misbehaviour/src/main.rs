//! A program that verifies a misbehaviour evidence.

#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings)]
#![allow(clippy::no_mangle_with_rust_abi)]
// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolValue;
use ibc_client_tendermint::types::{Header, Misbehaviour};
use ibc_eureka_solidity_types::msgs::IICS07TendermintMsgs::{
    ClientState as SolClientState, MisbehaviourOutput as SolMisbehaviourOutput,
};
use ibc_proto::{
    ibc::lightclients::tendermint::v1::{Header as RawHeader, Misbehaviour as RawMisbehaviour}, 
    Protobuf
};
use tendermint_light_client_misbehaviour::ClientState;
use ibc_core_client_types::Height;

/// Convert from Solidity ClientState to core ClientState
fn from_sol_client_state(cs: SolClientState) -> ClientState {
    ClientState {
        chain_id: cs.chainId,
        trust_level_numerator: cs.trustLevel.numerator,
        trust_level_denominator: cs.trustLevel.denominator,
        trusting_period_seconds: cs.trustingPeriod,
        unbonding_period_seconds: cs.unbondingPeriod,
        max_clock_drift_seconds: cs.maxClockDrift,
        frozen_height: if cs.frozenHeight.revisionHeight > 0 {
            Some(Height::new(cs.frozenHeight.revisionNumber, cs.frozenHeight.revisionHeight).unwrap())
        } else {
            None
        },
        latest_height: Height::new(cs.latestHeight.revisionNumber, cs.latestHeight.revisionHeight).unwrap(),
    }
}

/// The main function of the program.
///
/// # Panics
/// Panics if the verification fails.
pub fn main() {
    let encoded_1 = sp1_zkvm::io::read_vec();
    let encoded_2 = sp1_zkvm::io::read_vec();

    // input 1: client state
    let sol_client_state = SolClientState::abi_decode(&encoded_1).unwrap();
    let client_state = from_sol_client_state(sol_client_state);
    
    // input 2: the misbehaviour evidence
    let misbehaviour = <Misbehaviour as Protobuf<RawMisbehaviour>>::decode_vec(&encoded_2).unwrap();
    
    // Extract headers from misbehaviour
    let header_1 = misbehaviour.header1().clone();
    let header_2 = misbehaviour.header2().clone();

    let output = tendermint_light_client_misbehaviour::verify_misbehaviour(
        client_state,
        header_1,
        header_2,
    );
    
    // Convert to Solidity output format
    let sol_output = SolMisbehaviourOutput {
        clientStateCommitment: vec![].into(), // Will be computed by the contract
    };

    sp1_zkvm::io::commit_slice(&sol_output.abi_encode());
}