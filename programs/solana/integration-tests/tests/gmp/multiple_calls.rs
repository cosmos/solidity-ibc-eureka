use super::*;

/// Two GMP calls from the same sender: the `UserCounter` state accumulates
/// across both deliveries and `CounterAppState.total_counters` stays at 1.
#[tokio::test]
async fn test_multiple_gmp_calls() {
    let user = User::new();
    let relayer = Relayer::new();
    let admin = Admin::new();
    let proof_data = vec![0u8; 32];

    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });

    let gmp_account_pda = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    chain_b.prefund_lamports(gmp_account_pda, 10_000_000);

    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_b.counter_app_state_pda();

    chain_a.start().await;
    chain_b.start().await;

    // ── First call: increment by 42 ──
    let first_amount = 42u64;
    let first_payload = gmp::encode_increment_payload(
        counter_app_state,
        user_counter_pda,
        gmp_account_pda,
        first_amount,
    );
    let first_packet = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &first_payload);

    user.send_call(
        &mut chain_a,
        GmpSendCallParams {
            sequence: 1,
            timeout_timestamp: GMP_TIMEOUT,
            receiver: &test_gmp_app::ID.to_string(),
            payload: first_payload.encode_to_vec(),
        },
    )
    .await
    .expect("first send_call failed");

    let (b_payload_1, b_proof_1) = relayer
        .upload_chunks(&mut chain_b, 1, &first_packet, &proof_data)
        .await
        .expect("upload first recv chunks failed");

    let remaining = gmp::build_increment_remaining_accounts(
        gmp_account_pda,
        counter_app_state,
        user_counter_pda,
    );

    let recv_1 = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence: 1,
                payload_chunk_pda: b_payload_1,
                proof_chunk_pda: b_proof_1,
                remaining_accounts: remaining.clone(),
            },
        )
        .await
        .expect("first recv_packet failed");

    let ack_1_data = extract_ack_data(&chain_b, recv_1.ack_pda).await;

    let (a_payload_1, a_proof_1) = relayer
        .upload_chunks(&mut chain_a, 1, &first_packet, &proof_data)
        .await
        .expect("upload first ack chunks failed");

    relayer
        .gmp_ack_packet(
            &mut chain_a,
            GmpAckPacketParams {
                sequence: 1,
                acknowledgement: ack_1_data,
                payload_chunk_pda: a_payload_1,
                proof_chunk_pda: a_proof_1,
            },
        )
        .await
        .expect("first ack_packet failed");

    // ── Second call: increment by 58 ──
    let second_amount = 58u64;
    let second_payload = gmp::encode_increment_payload(
        counter_app_state,
        user_counter_pda,
        gmp_account_pda,
        second_amount,
    );
    let second_packet = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &second_payload);

    user.send_call(
        &mut chain_a,
        GmpSendCallParams {
            sequence: 2,
            timeout_timestamp: GMP_TIMEOUT,
            receiver: &test_gmp_app::ID.to_string(),
            payload: second_payload.encode_to_vec(),
        },
    )
    .await
    .expect("second send_call failed");

    let (b_payload_2, b_proof_2) = relayer
        .upload_chunks(&mut chain_b, 2, &second_packet, &proof_data)
        .await
        .expect("upload second recv chunks failed");

    let recv_2 = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence: 2,
                payload_chunk_pda: b_payload_2,
                proof_chunk_pda: b_proof_2,
                remaining_accounts: remaining,
            },
        )
        .await
        .expect("second recv_packet failed");

    let ack_2_data = extract_ack_data(&chain_b, recv_2.ack_pda).await;

    let (a_payload_2, a_proof_2) = relayer
        .upload_chunks(&mut chain_a, 2, &second_packet, &proof_data)
        .await
        .expect("upload second ack chunks failed");

    relayer
        .gmp_ack_packet(
            &mut chain_a,
            GmpAckPacketParams {
                sequence: 2,
                acknowledgement: ack_2_data,
                payload_chunk_pda: a_payload_2,
                proof_chunk_pda: a_proof_2,
            },
        )
        .await
        .expect("second ack_packet failed");

    // ── Verify accumulated state ──
    let user_counter = read_user_counter(&chain_b, user_counter_pda).await;
    assert_eq!(
        user_counter.count,
        first_amount.saturating_add(second_amount),
        "UserCounter should accumulate both increments"
    );

    let counter_state = read_counter_app_state(&chain_b, counter_app_state).await;
    assert_eq!(
        counter_state.total_counters, 1,
        "same user should still have only one counter"
    );

    for seq in [1u64, 2] {
        assert_gmp_result_exists(&chain_a, chain_a.client_id(), seq).await;
    }
}
