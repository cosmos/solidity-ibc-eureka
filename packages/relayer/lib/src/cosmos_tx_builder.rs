//! Common transaction building utilities for Cosmos SDK chains
//! This module consolidates shared functionality across cosmos-to-cosmos, cosmos-to-eth, and eth-to-cosmos modules

use anyhow::{anyhow, Result};
use ibc_proto_eureka::{
    cosmos::base::v1beta1::Coin,
    cosmos::tx::v1beta1::Fee,
    google::protobuf::Duration,
    ibc::core::client::v1::Height,
    ibc::lightclients::tendermint::v1::{ClientState, Fraction},
};

/// Default trust level for Tendermint light clients
#[must_use]
pub const fn default_trust_level() -> Fraction {
    Fraction {
        numerator: 1,
        denominator: 3,
    }
}

/// Default max clock drift for Tendermint light clients
#[must_use]
pub const fn default_max_clock_drift() -> Duration {
    Duration {
        seconds: 15,
        nanos: 0,
    }
}

/// Build a basic Tendermint client state
#[must_use]
pub fn build_tendermint_client_state(
    chain_id: String,
    latest_height: Height,
    trusting_period: Duration,
    unbonding_period: Duration,
) -> ClientState {
    ClientState {
        chain_id,
        trust_level: Some(default_trust_level()),
        trusting_period: Some(trusting_period),
        unbonding_period: Some(unbonding_period),
        max_clock_drift: Some(default_max_clock_drift()),
        latest_height: Some(latest_height),
        proof_specs: vec![], // Will be filled by the caller with ics23 specs
        upgrade_path: vec!["upgrade".to_string(), "upgradedIBCState".to_string()],
        ..Default::default()
    }
}

/// Build fee structure for Cosmos transactions
#[must_use]
pub fn build_fee(amount: u128, denom: String, gas_limit: u64) -> Fee {
    Fee {
        amount: vec![Coin {
            denom,
            amount: amount.to_string(),
        }],
        gas_limit,
        payer: String::new(),
        granter: String::new(),
    }
}

/// Extract chain ID from a client ID (e.g., `ibc_1` -> `ibc`)
///
/// # Errors
///
/// Returns an error if the client ID doesn't contain an underscore
pub fn extract_chain_id_from_client_id(client_id: &str) -> Result<String> {
    client_id
        .rsplit_once('_')
        .map(|(chain_id, _)| chain_id.to_string())
        .ok_or_else(|| anyhow!("invalid client id: {client_id}"))
}

/// Check if a height is valid (non-zero)
#[must_use]
pub fn is_valid_height(height: &Option<Height>) -> bool {
    height.as_ref().is_some_and(|h| h.revision_height > 0)
}
