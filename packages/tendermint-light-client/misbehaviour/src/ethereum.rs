//! Ethereum-specific implementations for misbehaviour detection

use crate::{MisbehaviourOutputInfo, check_for_misbehaviour_core};
use ibc_client_tendermint::types::{ConsensusState, Misbehaviour};
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::ClientState, IMisbehaviourMsgs::MisbehaviourOutput,
};

impl MisbehaviourOutputInfo<ClientState> for MisbehaviourOutput {
    fn from_misbehaviour_check(
        client_state: ClientState,
        misbehaviour: &Misbehaviour,
        trusted_consensus_state_1: ConsensusState,
        trusted_consensus_state_2: ConsensusState,
        time: u128,
    ) -> Self {
        MisbehaviourOutput {
            clientState: client_state,
            trustedHeight1: misbehaviour.header1().trusted_height.into(),
            trustedHeight2: misbehaviour.header2().trusted_height.into(),
            trustedConsensusState1: trusted_consensus_state_1.into(),
            trustedConsensusState2: trusted_consensus_state_2.into(),
            time,
        }
    }
}

/// The main function of the program without the zkVM wrapper.
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn check_for_misbehaviour(
    client_state: ClientState,
    misbehaviour: &Misbehaviour,
    trusted_consensus_state_1: ConsensusState,
    trusted_consensus_state_2: ConsensusState,
    time: u128,
) -> MisbehaviourOutput {
    check_for_misbehaviour_core(
        client_state,
        misbehaviour,
        trusted_consensus_state_1,
        trusted_consensus_state_2,
        time,
    )
}