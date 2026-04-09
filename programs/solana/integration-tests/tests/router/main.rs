//! Solana-to-Solana IBC router integration tests.
//!
//! Two independent chains run as separate `ProgramTest` instances. The
//! `Relayer` actor delivers packets between them while the `User` actor
//! initiates sends.
//!
//! The mock light client always accepts proofs, so these tests exercise the
//! full IBC router lifecycle (send -> recv -> ack) without real proof
//! verification.

use anchor_lang::AccountDeserialize;
use integration_tests::{
    admin::Admin,
    assert_commitment_set, assert_commitment_zeroed, assert_receipt_created,
    chain::{Chain, ChainConfig, Program},
    extract_ack_data, extract_custom_error,
    relayer::Relayer,
    router::{self, AckPacketParams, RecvPacketParams, SendPacketParams, TimeoutPacketParams},
    user::User,
    ASYNC_ACK_NOT_SUPPORTED, PACKET_COMMITMENT_MISMATCH,
};
use solana_ibc_types::ics24;
use solana_sdk::pubkey::Pubkey;

mod ack_after_timeout;
mod bidirectional;
mod double_ack;
mod double_timeout;
mod empty_ack;
mod error_ack;
mod full_lifecycle;
mod multi_chunk_proof;
mod proof_rejection;
mod recv_after_timeout;
mod replay;
mod sequential;
mod timeout;
mod timeout_after_ack;
mod unauthorized_relayer;

async fn read_app_state(
    chain: &Chain,
    app_state_pda: Pubkey,
) -> test_ibc_app::state::TestIbcAppState {
    let account = chain
        .get_account(app_state_pda)
        .await
        .expect("app state should exist");
    test_ibc_app::state::TestIbcAppState::try_deserialize(&mut &account.data[..])
        .expect("failed to deserialize app state")
}
