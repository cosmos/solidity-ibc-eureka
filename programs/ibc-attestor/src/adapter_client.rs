use std::{fmt::Debug, future::Future};
use thiserror::Error;

pub trait Signable: Sync + Send + borsh::BorshSerialize + borsh::BorshDeserialize + Debug {
    fn to_encoded_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let _ = self.serialize(&mut buf).unwrap();
        buf
    }
    fn height(&self) -> u64;
}

pub trait Adapter: Sync + Send + 'static {
    fn get_latest_finalized_block(
        &self,
    ) -> impl Future<Output = Result<impl Signable, AdapterError>> + Send;

    fn get_latest_unfinalized_block(
        &self,
    ) -> impl Future<Output = Result<impl Signable, AdapterError>> + Send;

    fn block_time_ms(&self) -> u64;
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Failed to fetch latest finalized block due to {0}")]
    FinalizedBlockError(String),
    #[error("Failed to fetch latest unfinalized block due to {0}")]
    UnfinalizedBlockError(String),
}
