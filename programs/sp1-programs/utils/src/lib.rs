//! Shared utilities for SP1 ICS07 Tendermint programs
//!
//! This module provides common conversion functions between Solidity types and
//! Tendermint light client types used across all SP1 programs.

#![deny(
    missing_docs,
    clippy::nursery,
    clippy::pedantic,
    warnings,
    unused_crate_dependencies
)]

use alloy_sol_types as _;
use ibc_client_tendermint::types::ConsensusState;
use ibc_core_client_types::Height;
use ibc_eureka_solidity_types::msgs::IICS07TendermintMsgs::{
    ClientState as SolClientState, ConsensusState as SolConsensusState,
};
use tendermint::Hash;
use tendermint_light_client_update_client::{ClientState, TrustThreshold};

/// Max clock drift in seconds
pub const MAX_CLOCK_DRIFT_SECONDS: u64 = 15;

/// Convert from Solidity `ClientState` to tendermint `ClientState`
///
/// # Panics
/// Panics if the heights are invalid (should not happen with validated Solidity input)
#[must_use]
pub fn to_tendermint_client_state(cs: &SolClientState) -> ClientState {
    ClientState {
        chain_id: cs.chainId.clone(),
        trust_level: TrustThreshold::new(
            cs.trustLevel.numerator.into(),
            cs.trustLevel.denominator.into(),
        ),
        trusting_period_seconds: cs.trustingPeriod.into(),
        unbonding_period_seconds: cs.unbondingPeriod.into(),
        max_clock_drift_seconds: MAX_CLOCK_DRIFT_SECONDS,
        is_frozen: cs.isFrozen,
        latest_height: Height::new(
            cs.latestHeight.revisionNumber,
            cs.latestHeight.revisionHeight,
        )
        .expect("valid latest height"),
    }
}

/// Convert from Solidity `ConsensusState` to tendermint `ConsensusState`
///
/// # Panics
/// Panics if the validators hash is not 32 bytes or timestamp is invalid
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "reverse operation of Time::from_unix_timestamp"
)]
pub fn to_tendermint_consensus_state(cs: &SolConsensusState) -> ConsensusState {
    use tendermint_light_client_verifier::types::Time;

    let total_nanos = cs.timestamp;
    let seconds = (total_nanos / 1_000_000_000) as i64;
    let nanos = (total_nanos % 1_000_000_000) as u32;

    let timestamp = Time::from_unix_timestamp(seconds, nanos).expect("valid timestamp");

    ConsensusState {
        root: cs.root.to_vec().into(),
        timestamp,
        next_validators_hash: Hash::Sha256(cs.nextValidatorsHash.0),
    }
}

/// Convert a Height to Solidity Height format
#[must_use]
pub fn to_sol_height(height: Height) -> ibc_eureka_solidity_types::msgs::IICS02ClientMsgs::Height {
    ibc_eureka_solidity_types::msgs::IICS02ClientMsgs::Height {
        revisionNumber: height.revision_number(),
        revisionHeight: height.revision_height(),
    }
}

/// Convert a tendermint `ConsensusState` to Solidity `ConsensusState` format
///
/// # Panics
/// Panics if the root or next validators hash have invalid lengths
#[must_use]
pub fn to_sol_consensus_state(cs: ConsensusState) -> SolConsensusState {
    let nanos_timestamp = cs.timestamp.unix_timestamp_nanos();

    SolConsensusState {
        timestamp: nanos_timestamp.try_into().expect("valid timestamp"),
        root: cs
            .root
            .into_vec()
            .as_slice()
            .try_into()
            .expect("root must be 32 bytes"),
        nextValidatorsHash: cs
            .next_validators_hash
            .as_bytes()
            .try_into()
            .expect("next validators hash must be 32 bytes"),
    }
}
