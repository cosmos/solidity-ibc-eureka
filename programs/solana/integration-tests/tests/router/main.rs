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
    chain::{mock_ibc_app_state_pda, mock_lc_accounts, Chain, ChainConfig, ChainProgram},
    deployer::Deployer,
    extract_ack_data, extract_custom_error,
    programs::{MockIbcApp, TestIbcApp},
    relayer::Relayer,
    router::{self, AckPacketParams, RecvPacketParams, SendPacketParams, TimeoutPacketParams},
    user::User,
    Actor, ASYNC_ACK_NOT_SUPPORTED, DUMMY_PROOF, PACKET_COMMITMENT_MISMATCH,
};
use solana_ibc_types::ics24;
use solana_sdk::transaction::Transaction;

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
mod three_chain;
mod timeout;
mod timeout_after_ack;
mod unauthorized_relayer;

async fn read_app_state(chain: &Chain) -> test_ibc_app::state::TestIbcAppState {
    let pda = router::test_ibc_app_state_pda();
    let account = chain
        .get_account(pda)
        .await
        .expect("app state should exist");
    test_ibc_app::state::TestIbcAppState::try_deserialize(&mut &account.data[..])
        .expect("failed to deserialize app state")
}
