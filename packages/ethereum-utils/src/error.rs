#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum EthereumUtilsError {
    #[error("failed to compute slot at timestamp with  \
        (timestamp ({timestamp}) - genesis ({genesis})) / seconds_per_slot ({seconds_per_slot}) + genesis_slot ({genesis_slot})"
    )]
    FailedToComputeSlotAtTimestamp {
        timestamp: u64,
        genesis: u64,
        seconds_per_slot: u64,
        genesis_slot: u64,
    },
}
