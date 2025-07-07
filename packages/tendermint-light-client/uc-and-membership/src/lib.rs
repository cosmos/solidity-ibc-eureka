//! The crate that contains the types and utilities for `tendermint-light-client-uc-and-membership` program.
#![deny(
    missing_docs,
    clippy::nursery,
    clippy::pedantic,
    warnings,
    unused_crate_dependencies
)]

use ibc_client_tendermint::types::{ConsensusState, Header};
use ibc_core_commitment_types::merkle::MerkleProof;
use tendermint_light_client_membership::{KVPair, MembershipOutput};
use tendermint_light_client_update_client::{ClientState, UpdateClientOutput};

/// Output from combined update client and membership verification
#[derive(Clone, Debug)]
pub struct UcAndMembershipOutput {
    /// Output from update client verification
    pub update_output: UpdateClientOutput,
    /// Output from membership verification
    pub membership_output: MembershipOutput,
}

/// Error type for combined update client and membership
#[derive(Debug, thiserror::Error)]
pub enum UcAndMembershipError {
    /// Invalid app hash
    #[error("invalid app hash: expected 32 bytes, got {0} bytes")]
    InvalidAppHash(usize),
    /// Update client error
    #[error("update client error: {0}")]
    UpdateClient(#[from] tendermint_light_client_update_client::UpdateClientError),
    /// Membership error
    #[error("membership error: {0}")]
    Membership(#[from] tendermint_light_client_membership::MembershipError),
}

/// IBC light client combined update of client and membership verification
#[must_use]
pub fn update_client_and_membership(
    client_state: ClientState,
    trusted_consensus_state: &ConsensusState,
    proposed_header: Header,
    time: u128,
    request_iter: impl Iterator<Item = (KVPair, MerkleProof)>,
) -> Result<UcAndMembershipOutput, UcAndMembershipError> {
    let app_hash_bytes = proposed_header.signed_header.header().app_hash.as_bytes();
    let app_hash: [u8; 32] = app_hash_bytes
        .try_into()
        .map_err(|_| UcAndMembershipError::InvalidAppHash(app_hash_bytes.len()))?;

    let uc_output = tendermint_light_client_update_client::update_client(
        client_state,
        trusted_consensus_state,
        proposed_header,
        time,
    )?;

    let mem_output = tendermint_light_client_membership::membership(app_hash, request_iter)?;

    Ok(UcAndMembershipOutput {
        update_output: uc_output,
        membership_output: mem_output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tendermint_light_client_update_client::trustthreshold;

    fn test_client_state() -> clientstate {
        clientstate {
            chain_id: "test-chain".to_string(),
            trust_level: trustthreshold::new(1, 3),
            trusting_period_seconds: 3600,
            unbonding_period_seconds: 7200,
            max_clock_drift_seconds: 60,
            frozen_height: none,
            latest_height: ibc_core_client_types::height::new(1, 100).unwrap(),
        }
    }

    #[test]
    fn test_client_state_fields() {
        let client_state = test_client_state();
        assert_eq!(client_state.chain_id, "test-chain");
        assert_eq!(client_state.trust_level.numerator, 1);
        assert_eq!(client_state.trust_level.denominator, 3);
        assert_eq!(client_state.trusting_period_seconds, 3600);
        assert_eq!(client_state.unbonding_period_seconds, 7200);
        assert_eq!(client_state.max_clock_drift_seconds, 60);
        assert!(client_state.frozen_height.is_none());
        assert_eq!(client_state.latest_height.revision_number(), 1);
        assert_eq!(client_state.latest_height.revision_height(), 100);
    }

    #[test]
    fn test_kv_pair_creation() {
        let kv = KVPair::new(b"test_key".to_vec(), b"test_value".to_vec());
        assert_eq!(kv.path, b"test_key");
        assert_eq!(kv.value, b"test_value");
        assert!(!kv.is_non_membership());

        let non_membership_kv = KVPair::new(b"test_key".to_vec(), vec![]);
        assert!(non_membership_kv.is_non_membership());
    }
}

