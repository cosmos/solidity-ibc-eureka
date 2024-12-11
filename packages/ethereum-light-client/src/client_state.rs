//! This module defines [`ClientState`].

use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::types::{fork_parameters::ForkParameters, height::Height};

/// The ethereum client state
#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
pub struct ClientState {
    /// The chain ID
    #[serde_as(as = "DisplayFromStr")]
    pub chain_id: u64,
    /// The genesis validators root
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub genesis_validators_root: B256,
    /// The minimum number of participants in the sync committee
    pub min_sync_committee_participants: u64, // TODO: Needs be added to e2e tests #143
    /// The time of genesis
    pub genesis_time: u64,
    /// The fork parameters
    pub fork_parameters: ForkParameters,
    /// The slot duration in seconds
    pub seconds_per_slot: u64,
    /// The number of slots per epoch
    pub slots_per_epoch: u64,
    /// The number of epochs per sync committee period
    pub epochs_per_sync_committee_period: u64,
    /// The latest slot of this client
    pub latest_slot: u64,
    /// The height at which the client was frozen
    // TODO: Should this be frozen_slot? Consider this in #143
    pub frozen_height: Height,
    /// The address of the IBC contract being tracked on Ethereum
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub ibc_contract_address: Address,
    /// The storage slot of the IBC commitment in the Ethereum contract
    #[serde(with = "ethereum_utils::base64::uint256")]
    pub ibc_commitment_slot: U256,
}
