//! Solana-to-Solana GMP integration tests.
//!
//! Two independent chains run as separate `ProgramTest` instances. The
//! `Relayer` actor bridges packets between them while the `User` actor
//! initiates GMP calls via `send_call`.
//!
//! The mock light client always accepts proofs, so these tests exercise the
//! full GMP lifecycle (`send_call` -> `recv_packet` -> `ack_packet`) without
//! real proof verification.

use anchor_lang::AccountDeserialize;
use ics27_gmp::errors::GMPError;
use integration_tests::{
    anchor_error_code,
    chain::{Chain, ChainConfig, IbcApp, TEST_CLOCK_TIME},
    extract_custom_error, gmp,
    gmp::{GmpAckPacketParams, GmpRecvPacketParams, GmpSendCallParams},
    relayer::Relayer,
    user::User,
    Actor,
};
use prost::Message as ProstMessage;

mod bidirectional;
mod direct_call_rejected;
mod failed_execution;
mod full_lifecycle;
mod multi_user;
mod multiple_calls;
mod prefunded_pda;
mod signer_exploit;
mod three_chain;
mod timeout;
mod timeout_too_long;
mod unauthorized_cpi;

/// GMP timeout must match `router::test_timeout(TEST_CLOCK_TIME)` so that
/// the commitment computed by `send_call` agrees with the ack/recv packet builders.
const GMP_TIMEOUT: u64 = TEST_CLOCK_TIME as u64 + 86_000;
/// GMP `send_call` with a timeout exceeding `MAX_TIMEOUT_DURATION` is rejected.
const GMP_TIMEOUT_TOO_LONG: u64 = TEST_CLOCK_TIME as u64 + 86_400;
