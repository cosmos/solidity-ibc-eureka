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

use tendermint_light_client_uc_and_membership::UcAndMembershipOutput;
use tendermint_light_client_update_client::{ClientState, ConsensusState};
use tendermint_light_client_membership::KVPair;

use ibc_client_tendermint::types::Header;
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::{
        ClientState as SolClientState, ConsensusState as SolConsensusState,
        UcAndMembershipOutput as SolUcAndMembershipOutput,
    },
    IMembershipMsgs::KVPair as SolKVPair,
};
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
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

/// Convert from Solidity KVPair to core KVPair
fn from_sol_kvpair(kv: SolKVPair) -> KVPair {
    KVPair::new(kv.path.to_vec(), kv.value.to_vec())
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
    // encoded_5 is the number of key-value pairs we want to verify
    let encoded_5 = sp1_zkvm::io::read_vec();
    let request_len = u16::from_le_bytes(encoded_5.try_into().unwrap());
    assert!(request_len != 0);

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

    let membership_requests: Vec<_> = (0..request_len).map(|_| {
        // loop_encoded_1 is the key-value pair we want to verify the membership of
        let loop_encoded_1 = sp1_zkvm::io::read_vec();
        let sol_kv_pair = SolKVPair::abi_decode(&loop_encoded_1).unwrap();
        let kv_pair = from_sol_kvpair(sol_kv_pair);

        // loop_encoded_2 is the Merkle proof of the key-value pair
        let loop_encoded_2 = sp1_zkvm::io::read_vec();
        let merkle_proof = MerkleProof::decode_vec(&loop_encoded_2).unwrap();

        (kv_pair, merkle_proof)
    }).collect();

    let output = tendermint_light_client_uc_and_membership::verify_uc_and_membership(
        client_state,
        trusted_consensus_state,
        proposed_header,
        current_timestamp_nanos,
        membership_requests,
    );
    
    // Convert to Solidity output format
    let sol_output = SolUcAndMembershipOutput {
        clientStateCommitment: vec![].into(), // Will be computed by the contract
        consensusStateCommitment: vec![].into(), // Will be computed by the contract
        newHeight: ibc_eureka_solidity_types::msgs::IICS02ClientMsgs::Height {
            revisionNumber: output.update_output.new_client_state.latest_height.revision_number(),
            revisionHeight: output.update_output.new_client_state.latest_height.revision_height(),
        },
        newTimestamp: output.update_output.new_consensus_state.timestamp_nanos.try_into().unwrap(),
        membershipOutput: ibc_eureka_solidity_types::msgs::IMembershipMsgs::MembershipOutput {
            commitmentRoot: output.membership_output.commitment_root.into(),
            kvPairs: output.membership_output.verified_kv_pairs.into_iter()
                .map(|kv| SolKVPair {
                    path: kv.path.into(),
                    value: kv.value.into(),
                })
                .collect(),
        },
    };

    sp1_zkvm::io::commit_slice(&sol_output.abi_encode());
}