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
use ibc_eureka_solidity_types::msgs::IICS07TendermintMsgs::{
    ClientState as SolClientState, ConsensusState as SolConsensusState,
    UpdateClientOutput as SolUpdateClientOutput,
};
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use tendermint_light_client_update_client::{
    ClientState, ConsensusState, UpdateClientOutput,
};
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

/// Convert from Solidity ConsensusState to core ConsensusState
fn from_sol_consensus_state(cs: SolConsensusState) -> ConsensusState {
    ConsensusState {
        timestamp_nanos: cs.timestamp.try_into().unwrap(),
        app_hash: cs.root,
        next_validators_hash: cs.nextValidatorsHash,
    }
}

/// Convert from core UpdateClientOutput to Solidity UpdateClientOutput
fn to_sol_output(output: UpdateClientOutput) -> SolUpdateClientOutput {
    SolUpdateClientOutput {
        clientStateCommitment: vec![].into(), // Will be computed by the contract
        consensusStateCommitment: vec![].into(), // Will be computed by the contract
        newHeight: ibc_eureka_solidity_types::msgs::IICS02ClientMsgs::Height {
            revisionNumber: output.new_client_state.latest_height.revision_number(),
            revisionHeight: output.new_client_state.latest_height.revision_height(),
        },
        newTimestamp: output.new_consensus_state.timestamp_nanos.try_into().unwrap(),
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
    let client_state = from_sol_client_state(sol_client_state);
    
    // input 2: the trusted consensus state
    let sol_consensus_state = SolConsensusState::abi_decode(&encoded_2).unwrap();
    let trusted_consensus_state = from_sol_consensus_state(sol_consensus_state);
    
    // input 3: the proposed header
    let proposed_header = <Header as Protobuf<RawHeader>>::decode_vec(&encoded_3).unwrap();
    
    // input 4: time
    let current_timestamp_nanos = u128::from_le_bytes(encoded_4.try_into().unwrap());
    
    let output = tendermint_light_client_update_client::verify_header_update(
        client_state,
        trusted_consensus_state,
        proposed_header,
        current_timestamp_nanos,
    );
    let sol_output = to_sol_output(output);

    sp1_zkvm::io::commit_slice(&sol_output.abi_encode());
}