use std::future::Future;
use thiserror::Error;

use crate::header::Header;

pub trait L2Adapter: Sync + Send {
    fn get_latest_finalized_block(
        &self,
    ) -> impl Future<Output = Result<Header, L2AdapterClientError>> + Send;

    fn get_latest_unfinalized_block(
        &self,
    ) -> impl Future<Output = Result<Header, L2AdapterClientError>> + Send;
}

#[derive(Debug, Error)]
pub enum L2AdapterClientError {
    #[error("Failed to fetch latest finalized block due to {0}")]
    FinalizedBlockError(String),
    #[error("Failed to fetch latest unfinalized block due to {0}")]
    UnfinalizedBlockError(String),
}
