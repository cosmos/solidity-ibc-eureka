//! This module defines [`ConsensusState`] and [`TrustedConsensusState`].

use alloy_primitives::{FixedBytes, B256};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ethereum_types::consensus::sync_committee::SyncCommittee;

use crate::{error::EthereumIBCError, header::ActiveSyncCommittee, verify::BlsVerify};

/// The consensus state of the Ethereum light client corresponding to a finalized header
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Debug, Clone)]
pub struct ConsensusState {
    /// The slot number of the finalized header
    pub slot: u64,
    /// The state merkle root of the finalized header
    #[schemars(with = "String")]
    pub state_root: B256,
    /// The storage merkle root of the tracked contract at the finalized header
    #[schemars(with = "String")]
    pub storage_root: B256,
    /// The execution timestamp of the finalized header
    pub timestamp: u64,
    /// aggregate public key of current sync committee at the finalized header
    #[schemars(with = "String")]
    pub current_sync_committee: FixedBytes<48>,
    /// aggregate public key of next sync committee at the finalized header if known
    #[schemars(with = "String")]
    pub next_sync_committee: Option<FixedBytes<48>>,
}

/// The trusted consensus state of the Ethereum light client
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct TrustedConsensusState {
    /// The consensus state
    pub state: ConsensusState,
    /// Full sync committee data which corresponds to the aggregate key that we
    /// store at the client.
    ///
    /// This sync committee can either be the current sync committee or the next sync
    /// committee. That's because the verifier uses next or current sync committee's
    /// public keys to verify the signature against.
    sync_committee: ActiveSyncCommittee,
}

impl TrustedConsensusState {
    /// Creates a new trusted consensus state
    /// # Errors
    /// Returns an error if the untrusted sync committee does not match the trusted state
    pub fn new<V: BlsVerify>(
        trusted_state: ConsensusState,
        untrusted_sync_committee: ActiveSyncCommittee,
        bls_verifier: &V,
    ) -> Result<Self, EthereumIBCError> {
        let full_committee = match untrusted_sync_committee {
            ActiveSyncCommittee::Current(ref committee) => {
                ensure!(
                    committee.aggregate_pubkey == trusted_state.current_sync_committee,
                    EthereumIBCError::CurrenttSyncCommitteeMismatch {
                        expected: trusted_state.current_sync_committee,
                        found: committee.aggregate_pubkey
                    }
                );
                committee
            }
            ActiveSyncCommittee::Next(ref committee) => {
                let trusted_next_sync_committee = trusted_state
                    .next_sync_committee
                    .ok_or(EthereumIBCError::NextSyncCommitteeUnknown)?;
                ensure!(
                    committee.aggregate_pubkey == trusted_next_sync_committee,
                    EthereumIBCError::NextSyncCommitteeMismatch {
                        expected: trusted_next_sync_committee,
                        found: committee.aggregate_pubkey
                    }
                );
                committee
            }
        };

        let aggregate_pubkey = bls_verifier
            .aggregate(&full_committee.pubkeys)
            .map_err(|e| EthereumIBCError::BlsAggregateError(e.to_string()))?;
        ensure!(
            aggregate_pubkey == full_committee.aggregate_pubkey,
            EthereumIBCError::AggregatePubkeyMismatch {
                expected: aggregate_pubkey,
                found: full_committee.aggregate_pubkey
            }
        );

        Ok(Self {
            state: trusted_state,
            sync_committee: untrusted_sync_committee,
        })
    }

    /// Returns the finalized slot of the trusted consensus state
    #[must_use]
    pub const fn finalized_slot(&self) -> u64 {
        self.state.slot
    }

    /// Returns the current slot of the trusted consensus state if it is available
    #[must_use]
    pub const fn current_sync_committee(&self) -> Option<&SyncCommittee> {
        if let ActiveSyncCommittee::Current(committee) = &self.sync_committee {
            Some(committee)
        } else {
            None
        }
    }

    /// Returns the next sync committee if it is available
    #[must_use]
    pub const fn next_sync_committee(&self) -> Option<&SyncCommittee> {
        if let ActiveSyncCommittee::Next(committee) = &self.sync_committee {
            Some(committee)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::FixedBytes;
    use ethereum_types::consensus::bls::BlsPublicKey;

    // Simple mock verifier for testing
    struct MockBlsVerifier;

    impl BlsVerify for MockBlsVerifier {
        type Error = String;

        fn fast_aggregate_verify(
            &self,
            _public_keys: &[BlsPublicKey],
            _msg: B256,
            _signature: ethereum_types::consensus::bls::BlsSignature,
        ) -> Result<(), Self::Error> {
            Ok(()) // Always succeed for test
        }

        fn aggregate(&self, public_keys: &[BlsPublicKey]) -> Result<BlsPublicKey, Self::Error> {
            // For testing, return the first pubkey or a default one
            if public_keys.is_empty() {
                Ok(BlsPublicKey::default())
            } else {
                Ok(public_keys[0])
            }
        }
    }

    // Helper function to create a test sync committee
    fn create_test_sync_committee() -> SyncCommittee {
        let test_pubkey1: BlsPublicKey = FixedBytes::from([1u8; 48]);
        let test_pubkey2: BlsPublicKey = FixedBytes::from([2u8; 48]);
        let pubkeys = vec![test_pubkey1, test_pubkey2];

        SyncCommittee {
            pubkeys: pubkeys.clone(),
            aggregate_pubkey: pubkeys[0], // For test, we'll use first key as aggregate
        }
    }

    // Helper function to create a test consensus state
    fn create_test_consensus_state() -> ConsensusState {
        ConsensusState {
            slot: 1000,
            state_root: B256::from([3u8; 32]),
            storage_root: B256::from([4u8; 32]),
            timestamp: 1_234_567_890,
            current_sync_committee: FixedBytes::from([1u8; 48]),
            next_sync_committee: Some(FixedBytes::from([2u8; 48])),
        }
    }

    #[test]
    fn test_trusted_consensus_state_new_with_current_committee() {
        let consensus_state = create_test_consensus_state();
        let sync_committee = create_test_sync_committee();

        // Make sure aggregate_pubkey matches current_sync_committee in state
        let mut committee = sync_committee;
        committee.aggregate_pubkey = consensus_state.current_sync_committee;

        // Our mock verifier returns the first pubkey as aggregate, so make sure the first pubkey
        // matches what the committee's aggregate_pubkey is set to
        committee.pubkeys[0] = committee.aggregate_pubkey;

        let active_committee = ActiveSyncCommittee::Current(committee);
        let verifier = MockBlsVerifier;

        // Should succeed because pubkeys match
        let result =
            TrustedConsensusState::new(consensus_state.clone(), active_committee, &verifier);
        assert!(
            result.is_ok(),
            "Failed to create TrustedConsensusState with current committee"
        );

        let trusted_state = result.unwrap();
        assert_eq!(trusted_state.finalized_slot(), consensus_state.slot);
        assert!(trusted_state.current_sync_committee().is_some());
        assert!(trusted_state.next_sync_committee().is_none());
    }

    #[test]
    fn test_trusted_consensus_state_new_with_next_committee() {
        let consensus_state = create_test_consensus_state();
        let sync_committee = create_test_sync_committee();

        // Make sure aggregate_pubkey matches next_sync_committee in state
        let mut committee = sync_committee;
        committee.aggregate_pubkey = consensus_state.next_sync_committee.unwrap();

        // Our mock verifier returns the first pubkey as aggregate, so make sure the first pubkey
        // matches what the committee's aggregate_pubkey is set to
        committee.pubkeys[0] = committee.aggregate_pubkey;

        let active_committee = ActiveSyncCommittee::Next(committee);
        let verifier = MockBlsVerifier;

        // Should succeed because pubkeys match
        let result =
            TrustedConsensusState::new(consensus_state.clone(), active_committee, &verifier);
        assert!(
            result.is_ok(),
            "Failed to create TrustedConsensusState with next committee"
        );

        let trusted_state = result.unwrap();
        assert_eq!(trusted_state.finalized_slot(), consensus_state.slot);
        assert!(trusted_state.current_sync_committee().is_none());
        assert!(trusted_state.next_sync_committee().is_some());
    }

    #[test]
    fn test_current_sync_committee_mismatch() {
        let consensus_state = create_test_consensus_state();
        let sync_committee = create_test_sync_committee();

        // Create mismatch - use a different pubkey than what's in the state
        let mut committee = sync_committee;
        committee.aggregate_pubkey = FixedBytes::from([99u8; 48]);

        let active_committee = ActiveSyncCommittee::Current(committee);
        let verifier = MockBlsVerifier;

        // Should fail because pubkeys don't match
        let result = TrustedConsensusState::new(consensus_state, active_committee, &verifier);
        assert!(
            result.is_err(),
            "Should fail with current committee mismatch"
        );

        match result {
            Err(EthereumIBCError::CurrenttSyncCommitteeMismatch { .. }) => {
                // This is the expected error
            }
            _ => panic!("Expected CurrenttSyncCommitteeMismatch error"),
        }
    }

    #[test]
    fn test_next_sync_committee_mismatch() {
        let consensus_state = create_test_consensus_state();
        let sync_committee = create_test_sync_committee();

        // Create mismatch - use a different pubkey than what's in the state
        let mut committee = sync_committee;
        committee.aggregate_pubkey = FixedBytes::from([99u8; 48]);

        let active_committee = ActiveSyncCommittee::Next(committee);
        let verifier = MockBlsVerifier;

        // Should fail because pubkeys don't match
        let result = TrustedConsensusState::new(consensus_state, active_committee, &verifier);
        assert!(result.is_err(), "Should fail with next committee mismatch");

        match result {
            Err(EthereumIBCError::NextSyncCommitteeMismatch { .. }) => {
                // This is the expected error
            }
            _ => panic!("Expected NextSyncCommitteeMismatch error"),
        }
    }

    #[test]
    fn test_next_sync_committee_unknown() {
        // Create state with no next committee
        let mut consensus_state = create_test_consensus_state();
        consensus_state.next_sync_committee = None;

        let sync_committee = create_test_sync_committee();
        let active_committee = ActiveSyncCommittee::Next(sync_committee);
        let verifier = MockBlsVerifier;

        // Should fail because next committee is missing
        let result = TrustedConsensusState::new(consensus_state, active_committee, &verifier);
        assert!(result.is_err(), "Should fail with next committee unknown");

        match result {
            Err(EthereumIBCError::NextSyncCommitteeUnknown) => {
                // This is the expected error
            }
            _ => panic!("Expected NextSyncCommitteeUnknown error"),
        }
    }

    #[test]
    fn test_aggregate_pubkey_mismatch() {
        let consensus_state = create_test_consensus_state();
        let mut sync_committee = create_test_sync_committee();

        // Set up committee so the initial check passes but the aggregate verification fails
        sync_committee.aggregate_pubkey = consensus_state.current_sync_committee;

        // Our mock verifier will return the first pubkey as the aggregate
        // But let's make sure that's different from the committee's aggregate_pubkey
        sync_committee.pubkeys[0] = FixedBytes::from([42u8; 48]);

        let active_committee = ActiveSyncCommittee::Current(sync_committee);
        let verifier = MockBlsVerifier;

        // Should fail during aggregate pubkey verification
        let result = TrustedConsensusState::new(consensus_state, active_committee, &verifier);
        assert!(
            result.is_err(),
            "Should fail with aggregate pubkey mismatch"
        );

        match result {
            Err(EthereumIBCError::AggregatePubkeyMismatch { .. }) => {
                // This is the expected error
            }
            _ => panic!("Expected AggregatePubkeyMismatch error"),
        }
    }
}
