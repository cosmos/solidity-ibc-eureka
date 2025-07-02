//! Ethereum-specific implementations for the update client

use crate::{ClientStateInfo, UpdateClientOutputInfo, update_client_generic};
use ibc_client_tendermint::types::{ConsensusState, Header};
use ibc_core_client_types::Height;
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::ClientState, IUpdateClientMsgs::UpdateClientOutput,
};
use tendermint_light_client_verifier::types::TrustThreshold;

impl ClientStateInfo for ClientState {
    fn chain_id(&self) -> &str {
        &self.chainId
    }

    fn trust_level(&self) -> TrustThreshold {
        self.trustLevel.clone().into()
    }

    fn trusting_period(&self) -> u64 {
        self.trustingPeriod.into()
    }
}

impl UpdateClientOutputInfo<ClientState> for UpdateClientOutput {
    fn from_verification(
        client_state: ClientState,
        trusted_consensus_state: ConsensusState,
        new_consensus_state: ConsensusState,
        time: u128,
        trusted_height: Height,
        new_height: Height,
    ) -> Self {
        UpdateClientOutput {
            clientState: client_state,
            trustedConsensusState: trusted_consensus_state.into(),
            newConsensusState: new_consensus_state.into(),
            time,
            trustedHeight: trusted_height.into(),
            newHeight: new_height.into(),
        }
    }
}

/// The main function of the program without the zkVM wrapper.
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn update_client(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
) -> UpdateClientOutput {
    update_client_core(client_state, trusted_consensus_state, proposed_header, time)
}
