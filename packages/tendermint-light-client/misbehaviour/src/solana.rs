//! Solana-specific types and implementations for misbehaviour program

use ibc_client_tendermint::types::{ConsensusState, Misbehaviour};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::ClientId;
use tendermint_light_client_update_client::SolanaClientState;

use crate::MisbehaviourOutputInfo;

/// Input for the misbehaviour program on Solana
#[derive(Clone, Debug)]
pub struct SolanaMisbehaviourInput<'a> {
    /// The client state
    pub client_state: SolanaClientState,
    /// The misbehaviour evidence
    pub misbehaviour: &'a Misbehaviour,
    /// Trusted consensus state for header 1
    pub trusted_consensus_state_1: ConsensusState,
    /// Trusted consensus state for header 2
    pub trusted_consensus_state_2: ConsensusState,
    /// Current time in nanoseconds
    pub time: u128,
}

/// Output for the misbehaviour program on Solana
#[derive(Clone, Debug)]
pub struct SolanaMisbehaviourOutput {
    /// Whether misbehaviour was detected
    pub misbehaviour_detected: bool,
    /// The client state
    pub client_state: SolanaClientState,
    /// The height at which misbehaviour was detected
    pub misbehaviour_height: Height,
    /// The time of misbehaviour detection
    pub time: u64,
    /// Type of misbehaviour detected
    pub misbehaviour_type: Option<MisbehaviourType>,
}

/// Types of misbehaviour that can be detected
#[derive(Clone, Debug, PartialEq)]
pub enum MisbehaviourType {
    /// Two headers at the same height with different values
    DoubleSign,
    /// Time monotonicity violation
    TimeMonotonicityViolation,
}

// TODO: Very simple verification
impl MisbehaviourOutputInfo<SolanaClientState> for SolanaMisbehaviourOutput {
    fn from_misbehaviour_check(
        mut client_state: SolanaClientState,
        misbehaviour: &Misbehaviour,
        _trusted_consensus_state_1: ConsensusState,
        _trusted_consensus_state_2: ConsensusState,
        time: u128,
    ) -> Self {
        // Check if headers have same height (double sign) or time violation
        let misbehaviour_type = if misbehaviour.header1.height() == misbehaviour.header2.height() {
            Some(MisbehaviourType::DoubleSign)
        } else if misbehaviour.header1.signed_header.header.time >= misbehaviour.header2.signed_header.header.time {
            Some(MisbehaviourType::TimeMonotonicityViolation)
        } else {
            None
        };

        let misbehaviour_detected = misbehaviour_type.is_some();
        
        // Freeze the client at the misbehaviour height if detected
        let misbehaviour_height = misbehaviour.header1.height();
        if misbehaviour_detected {
            client_state.frozen_height = Some(misbehaviour_height.revision_height());
        }

        Self {
            misbehaviour_detected,
            client_state,
            misbehaviour_height,
            time: time as u64,
            misbehaviour_type,
        }
    }
}

/// Helper to check if a client is frozen
pub fn is_client_frozen(client_state: &SolanaClientState) -> bool {
    client_state.frozen_height.is_some()
}

/// Check for misbehaviour in the provided headers
pub fn check_for_misbehaviour(input: SolanaMisbehaviourInput) -> SolanaMisbehaviourOutput {
    use crate::check_for_misbehaviour_core;
    
    check_for_misbehaviour_core(
        input.client_state,
        input.misbehaviour,
        input.trusted_consensus_state_1,
        input.trusted_consensus_state_2,
        input.time,
    )
}
