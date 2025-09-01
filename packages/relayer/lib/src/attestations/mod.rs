//! Collection of shared functions for
//! running the relayer with attesatations

mod aggregator;
mod proof;

pub use aggregator::{
    config::{AttestorConfig, CacheConfig, Config},
    Aggregator,
};
pub use proof::*;
