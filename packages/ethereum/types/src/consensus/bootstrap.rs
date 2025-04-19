//! This module defines types related to the bootstrap endpoint of the Beacon API.

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use super::{light_client_header::LightClientHeader, sync_committee::SyncCommittee};

/// The light client bootstrap
#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
#[allow(clippy::module_name_repetitions)]
pub struct LightClientBootstrap {
    /// The light client header
    pub header: LightClientHeader,
    /// The current sync committee
    pub current_sync_committee: SyncCommittee,
    /// The branch of the current sync committee
    // TODO: Add back type safety after Deneb support is removed (#440)
    pub current_sync_committee_branch: Vec<B256>,
}
