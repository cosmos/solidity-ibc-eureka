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
    admin::Admin,
    anchor_error_code, assert_commitment_set, assert_commitment_zeroed, assert_receipt_created,
    chain::{Chain, ChainConfig, Program, TEST_CLOCK_TIME},
    extract_ack_data, extract_custom_error, gmp,
    gmp::{GmpAckPacketParams, GmpRecvPacketParams, GmpSendCallParams},
    relayer::Relayer,
    user::User,
    Actor,
};
use prost::Message as ProstMessage;
use solana_sdk::pubkey::Pubkey;

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

async fn read_user_counter(chain: &Chain, pda: Pubkey) -> test_gmp_app::state::UserCounter {
    let account = chain
        .get_account(pda)
        .await
        .expect("UserCounter should exist");
    test_gmp_app::state::UserCounter::try_deserialize(&mut &account.data[..])
        .expect("deserialize UserCounter")
}

async fn read_counter_app_state(
    chain: &Chain,
    pda: Pubkey,
) -> test_gmp_app::state::CounterAppState {
    let account = chain
        .get_account(pda)
        .await
        .expect("CounterAppState should exist");
    test_gmp_app::state::CounterAppState::try_deserialize(&mut &account.data[..])
        .expect("deserialize CounterAppState")
}

async fn assert_gmp_result_exists(chain: &Chain, client_id: &str, sequence: u64) {
    let (pda, _) = solana_ibc_types::GMPCallResult::pda(client_id, sequence, &ics27_gmp::ID);
    let account = chain
        .get_account(pda)
        .await
        .expect("GMPCallResult should exist");
    assert_eq!(account.owner, ics27_gmp::ID);
}
