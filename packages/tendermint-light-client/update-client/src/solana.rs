//! Solana-specific types and implementations for update-client program

use ibc_client_tendermint::types::{ClientState as TmClientState, ConsensusState};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::ClientId;
use tendermint_light_client_verifier::types::TrustThreshold;

use crate::{ClientStateInfo, UpdateClientOutputInfo};

/// Solana-specific client state wrapper
#[derive(Clone, Debug)]
pub struct SolanaClientState {
    /// The chain ID
    pub chain_id: String,
    /// Trust level fraction
    pub trust_level_numerator: u64,
    /// Trust level denominator
    pub trust_level_denominator: u64,
    /// Trusting period in seconds
    pub trusting_period: u64,
    /// Unbonding period in seconds
    pub unbonding_period: u64,
    /// Max clock drift in seconds
    pub max_clock_drift: u64,
    /// Latest height
    pub latest_height: Height,
    /// Frozen height (0 if not frozen)
    pub frozen_height: u64,
}

impl ClientStateInfo for SolanaClientState {
    fn chain_id(&self) -> &str {
        &self.chain_id
    }

    fn trust_level(&self) -> TrustThreshold {
        TrustThreshold::new(self.trust_level_numerator, self.trust_level_denominator)
            .expect("Invalid trust level")
    }

    fn trusting_period(&self) -> u64 {
        self.trusting_period
    }
}

/// Input for the update-client program on Solana
#[derive(Clone, Debug)]
pub struct SolanaUpdateClientInput {
    /// The client ID
    pub client_id: ClientId,
    /// The client state
    pub client_state: SolanaClientState,
    /// The trusted consensus state
    pub trusted_consensus_state: ConsensusState,
    /// The proposed header
    pub proposed_header: Vec<u8>,
}

/// Output for the update-client program on Solana
#[derive(Clone, Debug)]
pub struct SolanaUpdateClientOutput {
    /// Updated client state
    pub client_state: SolanaClientState,
    /// The trusted consensus state used for verification
    pub trusted_consensus_state: ConsensusState,
    /// The new consensus state
    pub new_consensus_state: ConsensusState,
    /// The time of the update
    pub time: u64,
    /// The trusted height
    pub trusted_height: Height,
    /// The new height
    pub new_height: Height,
}

impl UpdateClientOutputInfo<SolanaClientState> for SolanaUpdateClientOutput {
    fn from_verification(
        client_state: SolanaClientState,
        trusted_consensus_state: ConsensusState,
        new_consensus_state: ConsensusState,
        time: u128,
        trusted_height: Height,
        new_height: Height,
    ) -> Self {
        Self {
            client_state,
            trusted_consensus_state,
            new_consensus_state,
            time: time as u64,
            trusted_height,
            new_height,
        }
    }
}

/// Convert from Tendermint ClientState to Solana ClientState
impl From<TmClientState> for SolanaClientState {
    fn from(tm_state: TmClientState) -> Self {
        Self {
            chain_id: tm_state.chain_id.to_string(),
            trust_level_numerator: tm_state.trust_level.numerator(),
            trust_level_denominator: tm_state.trust_level.denominator(),
            trusting_period: tm_state.trusting_period.as_secs(),
            unbonding_period: tm_state.unbonding_period.as_secs(),
            max_clock_drift: tm_state.max_clock_drift.as_secs(),
            latest_height: tm_state.latest_height,
            frozen_height: tm_state.frozen_height.unwrap_or(0),
        }
    }
}
