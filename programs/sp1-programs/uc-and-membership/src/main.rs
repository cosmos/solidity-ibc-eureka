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

use tendermint_light_client_uc_and_membership::update_client_and_membership;
use tendermint_light_client_update_client::{ClientState, TrustThreshold};
use tendermint_light_client_membership::KVPair;

use ibc_client_tendermint::types::{ConsensusState, Header};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::{ClientState as SolClientState, ConsensusState as SolConsensusState},
    IMembershipMsgs::KVPair as SolKVPair,
    IUpdateClientAndMembershipMsgs::UcAndMembershipOutput as SolUcAndMembershipOutput,
};
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use ibc_core_client_types::Height;

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
        timestamp: ibc_core_client_types::timestamp::Timestamp::from_nanoseconds(cs.timestamp.try_into().unwrap())
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
    // encoded_5 is the number of key-value pairs we want to verify
    let encoded_5 = sp1_zkvm::io::read_vec();
    let request_len = u16::from_le_bytes(encoded_5.try_into().unwrap());
    assert!(request_len != 0);

    // input 1: the client state
    let sol_client_state = SolClientState::abi_decode(&encoded_1).unwrap();
    let client_state = from_sol_client_state(sol_client_state.clone());
    // input 2: the trusted consensus state
    let sol_consensus_state = SolConsensusState::abi_decode(&encoded_2).unwrap();
    let trusted_consensus_state = from_sol_consensus_state(sol_consensus_state.clone());
    // input 3: the proposed header
    let proposed_header = <Header as Protobuf<RawHeader>>::decode_vec(&encoded_3).unwrap();
    // input 4: time
    let time = u128::from_le_bytes(encoded_4.try_into().unwrap());

    let request_iter = (0..request_len).map(|_| {
        // loop_encoded_1 is the key-value pair we want to verify the membership of
        let loop_encoded_1 = sp1_zkvm::io::read_vec();
        let sol_kv_pair = SolKVPair::abi_decode(&loop_encoded_1).unwrap();
        let kv_pair = KVPair::new(sol_kv_pair.path.to_vec(), sol_kv_pair.value.to_vec());

        // loop_encoded_2 is the Merkle proof of the key-value pair
        let loop_encoded_2 = sp1_zkvm::io::read_vec();
        let merkle_proof = MerkleProof::decode_vec(&loop_encoded_2).unwrap();

        (kv_pair, merkle_proof)
    });

    let output = update_client_and_membership(
        client_state,
        trusted_consensus_state,
        proposed_header,
        time,
        request_iter,
    );

    // Convert output to Solidity format
    let sol_update_output = ibc_eureka_solidity_types::msgs::IUpdateClientMsgs::UpdateClientOutput {
        clientState: sol_client_state,
        trustedConsensusState: sol_consensus_state.into(),
        newConsensusState: ibc_eureka_solidity_types::msgs::IICS07TendermintMsgs::ConsensusState {
            timestamp: output.update_output.new_consensus_state.timestamp.nanoseconds().try_into().unwrap(),
            root: output.update_output.new_consensus_state.root.into_vec().try_into().expect("root must be 32 bytes"),
            nextValidatorsHash: output.update_output.new_consensus_state.next_validators_hash.as_bytes().try_into().expect("next validators hash must be 32 bytes"),
        }.into(),
        time,
        trustedHeight: output.update_output.trusted_height.into(),
        newHeight: output.update_output.new_client_state.latest_height.into(),
    };

    let sol_output = SolUcAndMembershipOutput {
        updateClientOutput: sol_update_output,
        kvPairs: output.membership_output.kv_pairs.into_iter()
            .map(|kv| SolKVPair {
                path: kv.path.into(),
                value: kv.value.into(),
            })
            .collect(),
    };

    sp1_zkvm::io::commit_slice(&sol_output.abi_encode());
}