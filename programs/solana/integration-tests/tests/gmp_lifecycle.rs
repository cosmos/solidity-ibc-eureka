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
use integration_tests::{
    chain::{Chain, ChainConfig, TEST_CLOCK_TIME},
    gmp::{self, GmpAckPacketParams, GmpRecvPacketParams, GmpSendCallParams},
    relayer::Relayer,
    user::User,
    Actor,
};
use prost::Message as ProstMessage;

/// GMP timeout must match `router::test_timeout(TEST_CLOCK_TIME)` so that
/// the commitment computed by `send_call` agrees with the ack/recv packet builders.
const GMP_TIMEOUT: u64 = TEST_CLOCK_TIME as u64 + 86_000;

/// Full GMP lifecycle: `send_call` on A -> `recv_packet` on B -> `ack_packet` on A.
///
/// Chain A sends a GMP call targeting `test_gmp_app::increment` on Chain B.
/// Chain B receives the packet, GMP CPIs into `test_gmp_app`, creating a
/// `UserCounter` PDA. Chain A receives the ack, creating a `GMPCallResultAccount`.
#[tokio::test]
async fn test_gmp_full_lifecycle() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let increment_amount = 42u64;

    // ── Build Chain A (sender chain, with GMP) ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: true,
    });
    chain_a.prefund(&user);

    // ── Build Chain B (receiver chain, with GMP + test_gmp_app) ──
    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: true,
    });

    // Derive GMP account PDA on Chain B and pre-fund it
    let gmp_account_pda = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    chain_b.prefund_lamports(gmp_account_pda, 10_000_000);

    // Derive target account PDAs on Chain B
    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_b
        .accounts
        .counter_app_state_pda
        .expect("GMP chain should have counter app state");

    // Build the GMP payload for test_gmp_app::increment
    let solana_payload = gmp::encode_increment_payload(
        counter_app_state,
        user_counter_pda,
        gmp_account_pda,
        increment_amount,
    );
    let gmp_packet_bytes =
        gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &solana_payload);

    // ── Start both chains ──
    chain_a.start().await;
    chain_b.start().await;

    // ──────────────────────────────────────────────────────────────────────
    // User sends GMP call on Chain A
    // ──────────────────────────────────────────────────────────────────────
    let commitment_pda = user
        .send_call(
            &mut chain_a,
            GmpSendCallParams {
                sequence,
                timeout_timestamp: GMP_TIMEOUT,
                receiver: &test_gmp_app::ID.to_string(),
                payload: solana_payload.encode_to_vec(),
            },
        )
        .await
        .expect("send_call on Chain A failed");

    // Verify commitment was created
    let commitment_account = chain_a
        .get_account(commitment_pda)
        .await
        .expect("commitment should exist on Chain A");
    assert_eq!(commitment_account.owner, ics26_router::ID);
    assert_ne!(
        &commitment_account.data[8..40],
        &[0u8; 32],
        "commitment should be non-zero after send"
    );

    // ──────────────────────────────────────────────────────────────────────
    // Relayer uploads chunks and delivers recv_packet to Chain B
    // ──────────────────────────────────────────────────────────────────────
    let (b_recv_payload, b_recv_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload recv chunks on Chain B failed");

    let remaining_accounts = gmp::build_increment_remaining_accounts(
        gmp_account_pda,
        counter_app_state,
        user_counter_pda,
    );

    let recv = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence,
                payload_chunk_pda: b_recv_payload,
                proof_chunk_pda: b_recv_proof,
                remaining_accounts,
            },
        )
        .await
        .expect("recv_packet on Chain B failed");

    // Verify receipt and ack on Chain B
    let receipt = chain_b
        .get_account(recv.receipt_pda)
        .await
        .expect("receipt should exist on Chain B");
    assert_eq!(receipt.owner, ics26_router::ID);

    let ack = chain_b
        .get_account(recv.ack_pda)
        .await
        .expect("ack should exist on Chain B");
    assert_eq!(ack.owner, ics26_router::ID);
    assert_ne!(&ack.data[8..40], &[0u8; 32]);

    // Verify UserCounter was created with correct count
    let user_counter_account = chain_b
        .get_account(user_counter_pda)
        .await
        .expect("UserCounter should exist on Chain B");
    assert_eq!(user_counter_account.owner, test_gmp_app::ID);
    let user_counter =
        test_gmp_app::state::UserCounter::try_deserialize(&mut &user_counter_account.data[..])
            .expect("failed to deserialize UserCounter");
    assert_eq!(
        user_counter.count, increment_amount,
        "UserCounter should have count == increment_amount"
    );

    // Verify CounterAppState was updated
    let counter_state_account = chain_b
        .get_account(counter_app_state)
        .await
        .expect("CounterAppState should exist");
    let counter_state = test_gmp_app::state::CounterAppState::try_deserialize(
        &mut &counter_state_account.data[..],
    )
    .expect("failed to deserialize CounterAppState");
    assert_eq!(counter_state.total_counters, 1);

    // ──────────────────────────────────────────────────────────────────────
    // Relayer uploads chunks and delivers ack_packet back to Chain A
    // ──────────────────────────────────────────────────────────────────────
    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload ack chunks on Chain A failed");

    let ack_data = ack.data[8..40].to_vec();

    let ack_commitment_pda = relayer
        .gmp_ack_packet(
            &mut chain_a,
            GmpAckPacketParams {
                sequence,
                acknowledgement: ack_data,
                payload_chunk_pda: a_ack_payload,
                proof_chunk_pda: a_ack_proof,
            },
        )
        .await
        .expect("ack_packet on Chain A failed");

    // Verify commitment was zeroed
    let commitment = chain_a
        .get_account(ack_commitment_pda)
        .await
        .expect("commitment PDA should still exist on Chain A");
    assert_eq!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be zeroed after ack"
    );

    // Verify GMPCallResultAccount was created
    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(chain_a.client_id(), sequence, &ics27_gmp::ID);
    let result_account = chain_a
        .get_account(result_pda)
        .await
        .expect("GMPCallResultAccount should exist on Chain A");
    assert_eq!(result_account.owner, ics27_gmp::ID);
}

/// GMP timeout lifecycle: `send_call` on A -> `timeout_packet` on A.
///
/// Chain A sends a GMP call but the packet is never delivered to Chain B.
/// The relayer delivers a timeout back to Chain A, which creates a
/// `GMPCallResultAccount` with timeout status.
#[tokio::test]
async fn test_gmp_timeout() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let increment_amount = 42u64;

    // ── Build Chain A (sender chain, with GMP) ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: true,
    });
    chain_a.prefund(&user);

    // Build a GMP payload (same encoding as the full lifecycle test)
    let gmp_account_pda = gmp::derive_gmp_account_pda("chain-b-client", &user.pubkey());
    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_a
        .accounts
        .counter_app_state_pda
        .expect("GMP chain should have counter app state");

    let solana_payload = gmp::encode_increment_payload(
        counter_app_state,
        user_counter_pda,
        gmp_account_pda,
        increment_amount,
    );
    let gmp_packet_bytes =
        gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &solana_payload);

    // ── Start chain ──
    chain_a.start().await;

    // ── User sends GMP call on Chain A ──
    let commitment_pda = user
        .send_call(
            &mut chain_a,
            GmpSendCallParams {
                sequence,
                timeout_timestamp: GMP_TIMEOUT,
                receiver: &test_gmp_app::ID.to_string(),
                payload: solana_payload.encode_to_vec(),
            },
        )
        .await
        .expect("send_call failed");

    // Verify commitment was created
    let commitment = chain_a
        .get_account(commitment_pda)
        .await
        .expect("commitment should exist");
    assert_ne!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be non-zero after send"
    );

    // ── Relayer uploads chunks and delivers timeout on Chain A ──
    let (timeout_payload, timeout_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload timeout chunks on Chain A failed");
    let timeout_commitment_pda = relayer
        .gmp_timeout_packet(
            &mut chain_a,
            gmp::GmpTimeoutPacketParams {
                sequence,
                payload_chunk_pda: timeout_payload,
                proof_chunk_pda: timeout_proof,
            },
        )
        .await
        .expect("gmp_timeout_packet failed");

    // Verify commitment was zeroed
    let commitment = chain_a
        .get_account(timeout_commitment_pda)
        .await
        .expect("commitment PDA should still exist");
    assert_eq!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be zeroed after timeout"
    );

    // Verify GMPCallResultAccount was created with timeout status
    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(chain_a.client_id(), sequence, &ics27_gmp::ID);
    let result_account = chain_a
        .get_account(result_pda)
        .await
        .expect("GMPCallResultAccount should exist");
    assert_eq!(result_account.owner, ics27_gmp::ID);

    let result_state = ics27_gmp::state::GMPCallResultAccount::try_deserialize(
        &mut &result_account.data[..],
    )
    .expect("failed to deserialize GMPCallResultAccount");
    assert_eq!(
        result_state.status,
        solana_ibc_types::CallResultStatus::Timeout
    );
}
