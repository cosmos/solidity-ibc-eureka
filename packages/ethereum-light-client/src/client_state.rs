use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::types::{fork_parameters::ForkParameters, height::Height};

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct ClientState {
    #[serde_as(as = "DisplayFromStr")]
    pub chain_id: u64,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub genesis_validators_root: B256,
    pub min_sync_committee_participants: u64, // TODO: Needs be added to e2e tests #143
    pub genesis_time: u64,
    pub fork_parameters: ForkParameters,
    pub seconds_per_slot: u64,
    pub slots_per_epoch: u64,
    pub epochs_per_sync_committee_period: u64,
    pub latest_slot: u64,
    // TODO: Should this be frozen_slot? Consider this in #143
    pub frozen_height: Height,
    #[serde(with = "ethereum_utils::base64::uint256")]
    pub ibc_commitment_slot: U256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub ibc_contract_address: Address,
}
