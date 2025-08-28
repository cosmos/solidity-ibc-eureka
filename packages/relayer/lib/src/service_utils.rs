//! Common service patterns and utilities for relayer modules
//! This module provides shared functionality for `RelayerService` implementations

use tendermint::Hash;
use tonic::Status;

/// Convert `anyhow::Error` to `tonic::Status`
#[inline]
#[must_use]
pub fn to_tonic_status(err: anyhow::Error) -> Status {
    Status::from_error(err.into())
}

/// Parse Cosmos transaction hashes from request
///
/// # Errors
///
/// Returns a `Status` error if any transaction ID cannot be parsed as a valid hash
#[inline]
#[allow(clippy::result_large_err)]
pub fn parse_cosmos_tx_hashes(tx_ids: Vec<Vec<u8>>) -> Result<Vec<Hash>, Status> {
    tx_ids
        .into_iter()
        .map(Hash::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Status::from_error(e.into()))
}

/// Parse Ethereum transaction hashes from request
///
/// # Errors
///
/// Returns a `Status` error if any transaction ID is not exactly 32 bytes
#[inline]
#[allow(clippy::result_large_err)]
pub fn parse_eth_tx_hashes(tx_ids: Vec<Vec<u8>>) -> Result<Vec<[u8; 32]>, Status> {
    tx_ids
        .into_iter()
        .map(|tx_id| {
            tx_id
                .try_into()
                .map_err(|tx| format!("invalid tx hash: {tx:?}"))
        })
        .collect::<Result<Vec<[u8; 32]>, _>>()
        .map_err(|e| Status::from_error(e.into()))
}
