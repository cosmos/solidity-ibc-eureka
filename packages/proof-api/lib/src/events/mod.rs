//! Contains the `EurekaEvent` type, which is used to parse Cosmos SDK and EVM IBC Eureka events.

pub mod cosmos_sdk;
mod eureka;
#[cfg(feature = "solana")]
pub mod solana;

pub use eureka::{EurekaEvent, EurekaEventWithHeight};
#[cfg(feature = "solana")]
pub use solana::{SolanaEurekaEvent, SolanaEurekaEventWithHeight};
