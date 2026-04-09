//! `AccessManager` transfer integration tests for ICS26 Router and ICS27 GMP.
//!
//! Each test loads a `TestAccessManager` alongside the default `access_manager`.
//! The admin proposes transferring the AM on a target program (ICS26/GMP) to the
//! test AM, then the test AM's admin accepts (or the original admin cancels).
//!
//! ## Coverage gaps (not testable at integration level)
//!
//! - **ICS07 Tendermint AM transfer**: the integration test crate does not load
//!   `ics07-tendermint`; it uses `mock_light_client` exclusively.
//! - **AM-to-AM upgrade authority migration**: requires buffer accounts with
//!   real ELF binaries; tested in per-program `ProgramTest` and e2e.

use access_manager::AccessManagerError;
use integration_tests::{
    admin::Admin,
    anchor_error_code,
    chain::{Chain, ChainConfig, Program},
    extract_custom_error, gmp,
    relayer::Relayer,
    router,
};

mod am_transfer_gmp;
mod am_transfer_ics26;
