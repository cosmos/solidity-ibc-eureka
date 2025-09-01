//! Collection of shared functions for
//! running the relayer with attesatations

mod aggregator;
mod proof;

pub use aggregator::{
    aggregator::Aggregator,
    config::{AttestorConfig, CacheConfig, Config},
    rpc::GetAttestationsRequest,
};
pub use proof::*;
