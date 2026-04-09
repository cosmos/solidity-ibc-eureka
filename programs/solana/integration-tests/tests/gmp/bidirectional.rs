use super::*;

/// Both chains send GMP calls to each other. Each chain has an independent
/// `UserCounter` and `GMPCallResultAccount`.
#[tokio::test]
async fn test_gmp_bidirectional() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let amount_a_to_b = 10u64;
    let amount_b_to_a = 20u64;

    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_b.prefund(&user);

    // A→B: GMP account on B
    let gmp_pda_on_b = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    chain_b.prefund_lamports(gmp_pda_on_b, 10_000_000);
    let counter_on_b = gmp::derive_user_counter_pda(&gmp_pda_on_b);
    let counter_state_b = chain_b
        .accounts
        .counter_app_state_pda
        .expect("GMP chain should have counter app state");

    // B→A: GMP account on A
    let gmp_pda_on_a = gmp::derive_gmp_account_pda(chain_a.client_id(), &user.pubkey());
    chain_a.prefund_lamports(gmp_pda_on_a, 10_000_000);
    let counter_on_a = gmp::derive_user_counter_pda(&gmp_pda_on_a);
    let counter_state_a = chain_a
        .accounts
        .counter_app_state_pda
        .expect("GMP chain should have counter app state");

    // Build payloads
    let payload_a_to_b =
        gmp::encode_increment_payload(counter_state_b, counter_on_b, gmp_pda_on_b, amount_a_to_b);
    let packet_a_to_b = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &payload_a_to_b);

    let payload_b_to_a =
        gmp::encode_increment_payload(counter_state_a, counter_on_a, gmp_pda_on_a, amount_b_to_a);
    let packet_b_to_a = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &payload_b_to_a);

    chain_a.start().await;
    chain_b.start().await;

    // ── Send on both chains ──
    user.send_call(
        &mut chain_a,
        GmpSendCallParams {
            sequence,
            timeout_timestamp: GMP_TIMEOUT,
            receiver: &test_gmp_app::ID.to_string(),
            payload: payload_a_to_b.encode_to_vec(),
        },
    )
    .await
    .expect("send_call on A failed");

    user.send_call(
        &mut chain_b,
        GmpSendCallParams {
            sequence,
            timeout_timestamp: GMP_TIMEOUT,
            receiver: &test_gmp_app::ID.to_string(),
            payload: payload_b_to_a.encode_to_vec(),
        },
    )
    .await
    .expect("send_call on B failed");

    // ── Deliver A→B recv on Chain B ──
    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, &packet_a_to_b, &proof_data)
        .await
        .expect("upload A→B recv chunks failed");

    let remaining_b =
        gmp::build_increment_remaining_accounts(gmp_pda_on_b, counter_state_b, counter_on_b);
    let recv_on_b = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                proof_chunk_pda: b_proof,
                remaining_accounts: remaining_b,
            },
        )
        .await
        .expect("A→B recv_packet failed");

    // ── Deliver B→A recv on Chain A ──
    let (a_payload, a_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &packet_b_to_a, &proof_data)
        .await
        .expect("upload B→A recv chunks failed");

    let remaining_a =
        gmp::build_increment_remaining_accounts(gmp_pda_on_a, counter_state_a, counter_on_a);
    let recv_on_a = relayer
        .gmp_recv_packet(
            &mut chain_a,
            GmpRecvPacketParams {
                sequence,
                payload_chunk_pda: a_payload,
                proof_chunk_pda: a_proof,
                remaining_accounts: remaining_a,
            },
        )
        .await
        .expect("B→A recv_packet failed");

    // ── Deliver A→B ack back on Chain A ──
    // Clean up B→A recv chunks before uploading A→B ack chunks (same chain + sequence)
    relayer
        .cleanup_chunks(&mut chain_a, sequence, a_payload, a_proof)
        .await
        .expect("cleanup B→A recv chunks on A failed");
    let ack_b_data = chain_b
        .get_account(recv_on_b.ack_pda)
        .await
        .expect("A→B ack should exist")
        .data[8..40]
        .to_vec();
    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &packet_a_to_b, &proof_data)
        .await
        .expect("upload A→B ack chunks failed");
    relayer
        .gmp_ack_packet(
            &mut chain_a,
            GmpAckPacketParams {
                sequence,
                acknowledgement: ack_b_data,
                payload_chunk_pda: a_ack_payload,
                proof_chunk_pda: a_ack_proof,
            },
        )
        .await
        .expect("A→B ack_packet failed");

    // ── Deliver B→A ack back on Chain B ──
    // Clean up A→B recv chunks before uploading B→A ack chunks (same chain + sequence)
    relayer
        .cleanup_chunks(&mut chain_b, sequence, b_payload, b_proof)
        .await
        .expect("cleanup A→B recv chunks on B failed");
    let ack_a_data = chain_a
        .get_account(recv_on_a.ack_pda)
        .await
        .expect("B→A ack should exist")
        .data[8..40]
        .to_vec();
    let (b_ack_payload, b_ack_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, &packet_b_to_a, &proof_data)
        .await
        .expect("upload B→A ack chunks failed");
    relayer
        .gmp_ack_packet(
            &mut chain_b,
            GmpAckPacketParams {
                sequence,
                acknowledgement: ack_a_data,
                payload_chunk_pda: b_ack_payload,
                proof_chunk_pda: b_ack_proof,
            },
        )
        .await
        .expect("B→A ack_packet failed");

    // ── Verify independent state on each chain ──
    let counter_b_account = chain_b
        .get_account(counter_on_b)
        .await
        .expect("UserCounter on B should exist");
    let counter_b =
        test_gmp_app::state::UserCounter::try_deserialize(&mut &counter_b_account.data[..])
            .expect("deserialize UserCounter on B");
    assert_eq!(counter_b.count, amount_a_to_b);

    let counter_a_account = chain_a
        .get_account(counter_on_a)
        .await
        .expect("UserCounter on A should exist");
    let counter_a =
        test_gmp_app::state::UserCounter::try_deserialize(&mut &counter_a_account.data[..])
            .expect("deserialize UserCounter on A");
    assert_eq!(counter_a.count, amount_b_to_a);

    // GMPCallResultAccounts exist on both chains
    let (result_a, _) =
        solana_ibc_types::GMPCallResult::pda(chain_a.client_id(), sequence, &ics27_gmp::ID);
    assert!(chain_a.get_account(result_a).await.is_some());

    let (result_b, _) =
        solana_ibc_types::GMPCallResult::pda(chain_b.client_id(), sequence, &ics27_gmp::ID);
    assert!(chain_b.get_account(result_b).await.is_some());
}
