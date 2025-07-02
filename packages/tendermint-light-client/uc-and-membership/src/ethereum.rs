//! Ethereum-specific implementations for combined update client and membership

use crate::{update_client_and_membership_core, UcAndMembershipOutputInfo};
use ibc_client_tendermint_types::{ConsensusState, Header};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::ClientState,
    IMembershipMsgs::{KVPair, MembershipOutput},
    IUpdateClientAndMembershipMsgs::UcAndMembershipOutput,
    IUpdateClientMsgs::UpdateClientOutput,
};

impl UcAndMembershipOutputInfo<ClientState, KVPair> for UcAndMembershipOutput {
    type UpdateClientOutput = UpdateClientOutput;
    type MembershipOutput = MembershipOutput;

    fn from_results(
        uc_output: Self::UpdateClientOutput,
        membership_output: Self::MembershipOutput,
    ) -> Self {
        Self {
            updateClientOutput: uc_output,
            kvPairs: membership_output.kvPairs,
        }
    }
}

/// The main function of the program without the zkVM wrapper.
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn update_client_and_membership(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
    request_iter: impl Iterator<Item = (KVPair, MerkleProof)>,
) -> UcAndMembershipOutput {
    update_client_and_membership_core(
        client_state,
        trusted_consensus_state,
        proposed_header,
        time,
        request_iter,
    )
}
