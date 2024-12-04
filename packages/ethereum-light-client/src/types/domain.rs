use alloy_primitives::{hex, FixedBytes, B256};

use super::{fork_data::compute_fork_data_root, wrappers::Version};

pub struct DomainType(pub [u8; 4]);
impl DomainType {
    pub const BEACON_PROPOSER: Self = Self(hex!("00000000"));
    pub const BEACON_ATTESTER: Self = Self(hex!("01000000"));
    pub const RANDAO: Self = Self(hex!("02000000"));
    pub const DEPOSIT: Self = Self(hex!("03000000"));
    pub const VOLUNTARY_EXIT: Self = Self(hex!("04000000"));
    pub const SELECTION_PROOF: Self = Self(hex!("05000000"));
    pub const AGGREGATE_AND_PROOF: Self = Self(hex!("06000000"));
    pub const SYNC_COMMITTEE: Self = Self(hex!("07000000"));
    pub const SYNC_COMMITTEE_SELECTION_PROOF: Self = Self(hex!("08000000"));
    pub const CONTRIBUTION_AND_PROOF: Self = Self(hex!("09000000"));
    pub const BLS_TO_EXECUTION_CHANGE: Self = Self(hex!("0A000000"));
    pub const APPLICATION_MASK: Self = Self(hex!("00000001"));
}

/// Return the domain for the `domain_type` and `fork_version`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#compute_domain)
pub fn compute_domain(
    domain_type: DomainType,
    fork_version: Option<Version>,
    genesis_validators_root: Option<B256>,
    genesis_fork_version: Version,
) -> B256 {
    let fork_version = fork_version.unwrap_or(genesis_fork_version);
    let genesis_validators_root = genesis_validators_root.unwrap_or_default();
    let fork_data_root = compute_fork_data_root(fork_version, genesis_validators_root);

    let mut domain = [0; 32];
    domain[..4].copy_from_slice(&domain_type.0);
    domain[4..].copy_from_slice(&fork_data_root[..28]);

    FixedBytes(domain)
}
