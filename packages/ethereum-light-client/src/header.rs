//! This module defines types related to the Header we use for the Ethereum light client

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ethereum_types::{
    consensus::{light_client_header::LightClientUpdate, sync_committee::SyncCommittee},
    execution::account_proof::AccountProof,
};

/// The header of a light client update
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct Header {
    /// The trusted sync committee
    pub trusted_sync_committee: TrustedSyncCommittee,
    /// The consensus update
    pub consensus_update: LightClientUpdate,
    /// The account update
    pub account_update: AccountUpdate,
}

/// The account update
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct AccountUpdate {
    /// The account proof
    pub account_proof: AccountProof,
}

/// The trusted sync committee
// TODO: Could we use a enum to represent the current and next sync committee like
// `ActiveSyncCommittee`?
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
#[allow(clippy::module_name_repetitions)]
pub struct TrustedSyncCommittee {
    /// The trusted height
    pub trusted_slot: u64,
    // TODO:
    /// Active sync committee
    // pub active_sync_committee: ActiveSyncCommittee,
    /// The current sync committee
    pub current_sync_committee: Option<SyncCommittee>,
    /// The next sync committee
    pub next_sync_committee: Option<SyncCommittee>,
}

impl TrustedSyncCommittee {
    /// Returns the active sync committee
    // TODO: should this actually return default at any point? If not, panic or error
    // also, if not returning default, remove the impl Default
    #[must_use]
    pub fn get_active_sync_committee(&self) -> ActiveSyncCommittee {
        match (&self.current_sync_committee, &self.next_sync_committee) {
            (Some(sync_committee), _) => ActiveSyncCommittee::Current(sync_committee.clone()),
            (_, Some(sync_committee)) => ActiveSyncCommittee::Next(sync_committee.clone()),
            _ => ActiveSyncCommittee::default(),
        }
    }
}

/// The active sync committee
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum ActiveSyncCommittee {
    /// The current sync committee
    Current(SyncCommittee),
    /// The next sync committee
    Next(SyncCommittee),
}

impl Default for ActiveSyncCommittee {
    fn default() -> Self {
        Self::Current(SyncCommittee::default())
    }
}
