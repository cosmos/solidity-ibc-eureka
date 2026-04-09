use super::*;

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
        programs: &[Program::Gmp],
    });
    chain_a.prefund(&user);

    // ── Build Chain B (receiver chain, with GMP + test_gmp_app) ──
    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        programs: &[Program::Gmp],
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
    let counter_state =
        test_gmp_app::state::CounterAppState::try_deserialize(&mut &counter_state_account.data[..])
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
