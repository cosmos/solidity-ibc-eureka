use std::future::Future;
use thiserror::Error;

use crate::header::Header;

pub trait Adapter: Sync + Send {
    fn get_latest_finalized_block(
        &self,
    ) -> impl Future<Output = Result<Header, AdapterError>> + Send;

    fn get_latest_unfinalized_block(
        &self,
    ) -> impl Future<Output = Result<Header, AdapterError>> + Send;
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Failed to fetch latest finalized block due to {0}")]
    FinalizedBlockError(String),
    #[error("Failed to fetch latest unfinalized block due to {0}")]
    UnfinalizedBlockError(String),
}
