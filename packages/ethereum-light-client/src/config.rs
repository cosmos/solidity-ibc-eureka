//! The configuration module contains the configurations and presets for the beacon chains.

use core::{
    fmt::{self, Debug},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use typenum::{NonZero, Unsigned};

use alloy_primitives::FixedBytes;

/// Minimal config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Minimal;

/// Mainnet config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mainnet;

/// The base kind of the preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetBaseKind {
    /// Minimal preset
    Minimal,
    /// Mainnet preset
    Mainnet,
}

impl fmt::Display for PresetBaseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Minimal => "minimal",
            Self::Mainnet => "mainnet",
        })
    }
}

impl FromStr for PresetBaseKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "minimal" => Ok(Self::Minimal),
            "mainnet" => Ok(Self::Mainnet),
            _ => Err(s.to_string()),
        }
    }
}

// https://github.com/rust-lang/rust/issues/35853#issuecomment-415993963
macro_rules! with_dollar_sign {
    ($($body:tt)*) => {
        macro_rules! __with_dollar_sign { $($body)* }
        __with_dollar_sign!($);
    }
}

macro_rules! consts_traits {
    ($($CONST:ident $(,)?),+) => {
        $(
            #[allow(non_camel_case_types, missing_docs)]
            pub trait $CONST: Send + Sync + Unpin + 'static {
                // Extra traits are required because the builtin derives bound all generic
                // types unconditionally
                type $CONST: Unsigned + NonZero + Debug + Clone + PartialEq + Eq + Send + Sync + Unpin +  Default;
            }
        )+

        #[allow(missing_docs, clippy::trait_duplication_in_bounds)]
        pub trait ChainSpec: 'static + Debug + Clone + PartialEq + Eq + Default + Send + Sync + Unpin + Default + $($CONST+)+ {
            const PRESET: preset::Preset;
            // const PRESET_BASE_KIND: PresetBaseKind;

            type PERIOD: 'static + Unsigned;
        }

        with_dollar_sign! {
            ($d:tt) => {
                // TODO: Keep an eye on this issue https://github.com/rust-lang/rust/issues/98291, as it might resolve an issue with macro_export-ing this macro (currently it is only available in this crate)
                macro_rules! mk_chain_spec {
                    ($d T:ident is $d preset:path) => {
                        $(
                            impl $d crate::config:: $CONST for $d T {
                                #[allow(non_camel_case_types)]
                                type $CONST = $d crate::typenum::U<{ $d preset.$CONST }>;
                            }
                        )*

                        impl $d crate::config::ChainSpec for $d T {
                            const PRESET: $d crate::config::preset::Preset = $d preset;
                            // const PRESET_BASE_KIND: PresetBaseKind = PresetBaseKind::Mainnet;

                            type PERIOD = $d crate::typenum::Prod<
                                <Self as $d crate::config::EPOCHS_PER_SYNC_COMMITTEE_PERIOD>::EPOCHS_PER_SYNC_COMMITTEE_PERIOD,
                                <Self as $d crate::config::SLOTS_PER_EPOCH>::SLOTS_PER_EPOCH,
                            >;
                        }
                    };
                }
            }
        }
    };
}

consts_traits![
    // Misc
    DEPOSIT_CONTRACT_TREE_DEPTH,
    MAX_VALIDATORS_PER_COMMITTEE,
    // Time parameters
    SECONDS_PER_SLOT,
    SLOTS_PER_EPOCH,
    // Max operations per block
    MAX_PROPOSER_SLASHINGS,
    MAX_ATTESTER_SLASHINGS,
    MAX_ATTESTATIONS,
    MAX_DEPOSITS,
    MAX_VOLUNTARY_EXITS,
    MAX_BLS_TO_EXECUTION_CHANGES,
    MAX_BLOB_COMMITMENTS_PER_BLOCK,
    // Execution
    MAX_BYTES_PER_TRANSACTION,
    MAX_TRANSACTIONS_PER_PAYLOAD,
    BYTES_PER_LOGS_BLOOM,
    MAX_EXTRA_DATA_BYTES,
    MAX_WITHDRAWALS_PER_PAYLOAD,
    // Sync committee
    SYNC_COMMITTEE_SIZE,
    EPOCHS_PER_SYNC_COMMITTEE_PERIOD,
    // Sync protocol
    MIN_SYNC_COMMITTEE_PARTICIPANTS,
    UPDATE_TIMEOUT,
];

self::mk_chain_spec!(Minimal is preset::MINIMAL);
self::mk_chain_spec!(Mainnet is preset::MAINNET);

pub(crate) use mk_chain_spec;

use crate::types::{fork::Fork, fork_parameters::ForkParameters, wrappers::WrappedVersion};

/// Values that are constant across all configurations.
pub mod consts {
    /// <https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#get_subtree_index>
    #[must_use]
    pub const fn get_subtree_index(idx: u64) -> u64 {
        idx % 2_u64.pow(idx.ilog2())
    }

    /// Convenience function safely to call [`u64::ilog2`] and convert the result into a usize.
    #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
    #[must_use]
    pub const fn floorlog2(n: u64) -> usize {
        // conversion is safe since usize is either 32 or 64 bits as per cfg above
        n.ilog2() as usize
    }

    // https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#constants
    // REVIEW: Is it possible to implement get_generalized_index in const rust?

    // https://github.com/ethereum/consensus-specs/blob/dev/ssz/merkle-proofs.md
    /// `get_generalized_index(BeaconState, "finalized_checkpoint", "root")`
    pub const FINALIZED_ROOT_INDEX: u64 = 105;
    /// `get_generalized_index(BeaconState, "current_sync_committee")`
    pub const CURRENT_SYNC_COMMITTEE_INDEX: u64 = 54;
    /// `get_generalized_index(BeaconState, "next_sync_committee")`
    pub const NEXT_SYNC_COMMITTEE_INDEX: u64 = 55;
    /// `get_generalized_index(BeaconBlockBody, "execution_payload")`
    pub const EXECUTION_PAYLOAD_INDEX: u64 = 25;

    // Branch depths for different merkle trees related to ethereum consensus

    /// The depth of the merkle tree for execution payloads.
    pub const EXECUTION_BRANCH_DEPTH: usize = floorlog2(EXECUTION_PAYLOAD_INDEX);
    /// The depth of the merkle tree for the next sync committee.
    pub const NEXT_SYNC_COMMITTEE_BRANCH_DEPTH: usize = floorlog2(NEXT_SYNC_COMMITTEE_INDEX);
    /// The depth of the merkle tree for the finalized root.
    pub const FINALITY_BRANCH_DEPTH: usize = floorlog2(FINALIZED_ROOT_INDEX);
}

/// This module contains the preset values for the different configurations.
pub mod preset {
    /// The preset
    #[allow(non_snake_case)]
    #[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct Preset {
        // Misc
        // ---------------------------------------------------------------
        /// The depth of the deposit contract tree.
        pub DEPOSIT_CONTRACT_TREE_DEPTH: usize,
        /// The max number of validators per committee.
        pub MAX_VALIDATORS_PER_COMMITTEE: usize,

        // Time parameters
        // ---------------------------------------------------------------
        /// The number of seconds per slot.
        pub SECONDS_PER_SLOT: usize,
        /// The number of slots per epoch.
        pub SLOTS_PER_EPOCH: usize,

        // Max operations per block
        // ---------------------------------------------------------------
        /// The max number of proposer slashings per block.
        pub MAX_PROPOSER_SLASHINGS: usize,
        /// The max number of attester slashings per block.
        pub MAX_ATTESTER_SLASHINGS: usize,
        /// The max number of attestations per block.
        pub MAX_ATTESTATIONS: usize,
        /// The max number of deposits per block.
        pub MAX_DEPOSITS: usize,
        /// The max number of voluntary exits per block.
        pub MAX_VOLUNTARY_EXITS: usize,
        /// The max number of BLS to execution changes per block.
        pub MAX_BLS_TO_EXECUTION_CHANGES: usize,
        /// The max number of blob commitments per block.
        pub MAX_BLOB_COMMITMENTS_PER_BLOCK: usize,

        // Execution
        // ---------------------------------------------------------------
        /// The max number of bytes per transaction.
        pub MAX_BYTES_PER_TRANSACTION: usize,
        /// The max number of transactions per payload.
        pub MAX_TRANSACTIONS_PER_PAYLOAD: usize,
        /// The number of bytes per logs bloom.
        pub BYTES_PER_LOGS_BLOOM: usize,
        /// The max number of extra data bytes.
        pub MAX_EXTRA_DATA_BYTES: usize,
        /// The max number of withdrawals per payload.
        pub MAX_WITHDRAWALS_PER_PAYLOAD: usize,

        // Sync committee
        // ---------------------------------------------------------------
        /// The size of the sync committee.
        pub SYNC_COMMITTEE_SIZE: usize,
        /// The number of epochs per sync committee period.
        pub EPOCHS_PER_SYNC_COMMITTEE_PERIOD: usize,

        // Sync protocol
        // ---------------------------------------------------------------
        /// The min number of sync committee participants.
        pub MIN_SYNC_COMMITTEE_PARTICIPANTS: usize,
        /// The update timeout.
        pub UPDATE_TIMEOUT: usize,
    }

    /// <https://github.com/ethereum/consensus-specs/blob/dev/presets/mainnet>
    pub const MAINNET: Preset = Preset {
        DEPOSIT_CONTRACT_TREE_DEPTH: 32,
        MAX_VALIDATORS_PER_COMMITTEE: 2048,

        SECONDS_PER_SLOT: 12,
        SLOTS_PER_EPOCH: 32,

        MAX_PROPOSER_SLASHINGS: 16,
        MAX_ATTESTER_SLASHINGS: 2,
        MAX_ATTESTATIONS: 128,
        MAX_DEPOSITS: 16,
        MAX_VOLUNTARY_EXITS: 16,
        MAX_BLS_TO_EXECUTION_CHANGES: 16,
        MAX_BLOB_COMMITMENTS_PER_BLOCK: 4096,
        SYNC_COMMITTEE_SIZE: 512,
        EPOCHS_PER_SYNC_COMMITTEE_PERIOD: 256,
        MIN_SYNC_COMMITTEE_PARTICIPANTS: 1,
        UPDATE_TIMEOUT: 8192,

        MAX_BYTES_PER_TRANSACTION: 1_073_741_824,
        MAX_TRANSACTIONS_PER_PAYLOAD: 1_048_576,
        BYTES_PER_LOGS_BLOOM: 256,
        MAX_EXTRA_DATA_BYTES: 32,
        MAX_WITHDRAWALS_PER_PAYLOAD: 16,
    };

    /// <https://github.com/ethereum/consensus-specs/blob/dev/presets/minimal>
    pub const MINIMAL: Preset = Preset {
        DEPOSIT_CONTRACT_TREE_DEPTH: 32,
        MAX_VALIDATORS_PER_COMMITTEE: 2048,

        SECONDS_PER_SLOT: 6,
        SLOTS_PER_EPOCH: 8,

        MAX_PROPOSER_SLASHINGS: 16,
        MAX_ATTESTER_SLASHINGS: 2,
        MAX_ATTESTATIONS: 128,
        MAX_DEPOSITS: 16,
        MAX_VOLUNTARY_EXITS: 16,
        MAX_BLS_TO_EXECUTION_CHANGES: 16,
        MAX_BLOB_COMMITMENTS_PER_BLOCK: 16,

        SYNC_COMMITTEE_SIZE: 32,
        EPOCHS_PER_SYNC_COMMITTEE_PERIOD: 8,
        MIN_SYNC_COMMITTEE_PARTICIPANTS: 1,
        UPDATE_TIMEOUT: 64,

        MAX_BYTES_PER_TRANSACTION: 1_073_741_824,
        MAX_TRANSACTIONS_PER_PAYLOAD: 1_048_576,
        BYTES_PER_LOGS_BLOOM: 256,
        MAX_EXTRA_DATA_BYTES: 32,
        MAX_WITHDRAWALS_PER_PAYLOAD: 4,
    };
}

/// The configuration for the beacon chain.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// The preset
    pub preset: preset::Preset,
    /// The fork parameters
    pub fork_parameters: ForkParameters,
    /// The minimum genesis time
    pub min_genesis_time: u64,
}

/// The Goerli testnet configuration.
pub const GOERLI: Config = Config {
    preset: preset::MAINNET,
    fork_parameters: ForkParameters {
        genesis_fork_version: WrappedVersion(FixedBytes([0, 0, 16, 32])),
        genesis_slot: (0),
        altair: Fork {
            version: WrappedVersion(FixedBytes([1, 0, 16, 32])),
            epoch: (36660),
        },
        bellatrix: Fork {
            version: WrappedVersion(FixedBytes([2, 0, 16, 32])),
            epoch: 112_260,
        },
        capella: Fork {
            version: WrappedVersion(FixedBytes([3, 0, 16, 32])),
            epoch: 162_304,
        },
        deneb: Fork {
            version: WrappedVersion(FixedBytes([4, 0, 16, 32])),
            epoch: 231_680,
        },
    },
    min_genesis_time: 1_614_588_812,
};

/// The mainnet configuration.
pub const MAINNET: Config = Config {
    preset: preset::MAINNET,
    fork_parameters: ForkParameters {
        genesis_fork_version: WrappedVersion(FixedBytes([0, 0, 0, 0])),
        genesis_slot: 0,

        altair: Fork {
            version: WrappedVersion(FixedBytes([1, 0, 0, 0])),
            epoch: 74_240,
        },
        bellatrix: Fork {
            version: WrappedVersion(FixedBytes([2, 0, 0, 0])),
            epoch: 144_896,
        },
        capella: Fork {
            version: WrappedVersion(FixedBytes([3, 0, 0, 0])),
            epoch: 194_048,
        },
        // TODO: enabled march 13th 2024
        deneb: Fork {
            version: WrappedVersion(FixedBytes([4, 0, 0, 0])),
            epoch: u64::MAX,
        },
    },
    min_genesis_time: 1_606_824_000,
};

/// The minimal configuration.
pub const MINIMAL: Config = Config {
    preset: preset::MINIMAL,
    fork_parameters: ForkParameters {
        genesis_fork_version: WrappedVersion(FixedBytes([0, 0, 0, 1])),
        genesis_slot: 0,

        altair: Fork {
            version: WrappedVersion(FixedBytes([1, 0, 0, 1])),
            epoch: 0,
        },

        bellatrix: Fork {
            version: WrappedVersion(FixedBytes([2, 0, 0, 1])),
            epoch: 0,
        },

        capella: Fork {
            version: WrappedVersion(FixedBytes([3, 0, 0, 1])),
            epoch: 0,
        },

        // NOTE: dummy data
        deneb: Fork {
            version: WrappedVersion(FixedBytes([4, 0, 0, 1])),
            epoch: 0,
        },
    },
    min_genesis_time: 1_578_009_600,
};

/// The sepolia testnet configuration.
pub const SEPOLIA: Config = Config {
    preset: preset::MAINNET,
    fork_parameters: ForkParameters {
        genesis_fork_version: WrappedVersion(FixedBytes([144, 0, 0, 105])),
        genesis_slot: 0,

        altair: Fork {
            version: WrappedVersion(FixedBytes([144, 0, 0, 112])),
            epoch: 50,
        },

        bellatrix: Fork {
            version: WrappedVersion(FixedBytes([144, 0, 0, 113])),
            epoch: 100,
        },

        capella: Fork {
            version: WrappedVersion(FixedBytes([144, 0, 0, 114])),
            epoch: 56_832,
        },

        deneb: Fork {
            version: WrappedVersion(FixedBytes([144, 0, 0, 115])),
            epoch: 132_608,
        },
    },
    min_genesis_time: 1_655_647_200,
};
