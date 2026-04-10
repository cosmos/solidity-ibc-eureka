use super::*;

/// Both chains send GMP calls to each other. Each chain has an independent
/// `UserCounter` and `GMPCallResultAccount`.
#[tokio::test]
async fn test_gmp_bidirectional() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let sequence = 1u64;
    let amount_a_to_b = 10u64;
    let amount_b_to_a = 20u64;

    // ── Chains ──
    let programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &TestGmpApp];
    let (mut chain_a, mut chain_b) = Chain::pair(&deployer, programs);
    chain_a.prefund(&[&admin, &relayer, &user]);
    chain_b.prefund(&[&admin, &relayer, &user]);

    let gmp_pda_on_a = gmp::derive_gmp_account_pda(chain_a.client_id(), &user.pubkey());
    chain_a.prefund_lamports(gmp_pda_on_a, GMP_ACCOUNT_PREFUND_LAMPORTS);
    let gmp_pda_on_b = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    chain_b.prefund_lamports(gmp_pda_on_b, GMP_ACCOUNT_PREFUND_LAMPORTS);

    // ── Init ──
    chain_a.init(&deployer, &admin, &relayer, programs).await;
    chain_b.init(&deployer, &admin, &relayer, programs).await;

    // ── Build payloads ──
    let counter_on_a = gmp::derive_user_counter_pda(&gmp_pda_on_a);
    let counter_state_a = chain_a.counter_app_state_pda();
    let counter_on_b = gmp::derive_user_counter_pda(&gmp_pda_on_b);
    let counter_state_b = chain_b.counter_app_state_pda();

    let payload_a_to_b =
        gmp::encode_increment_payload(counter_state_b, counter_on_b, gmp_pda_on_b, amount_a_to_b);
    let packet_a_to_b = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &payload_a_to_b);

    let payload_b_to_a =
        gmp::encode_increment_payload(counter_state_a, counter_on_a, gmp_pda_on_a, amount_b_to_a);
    let packet_b_to_a = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &payload_b_to_a);

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
        .upload_chunks(&mut chain_b, sequence, &packet_a_to_b, DUMMY_PROOF)
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
        .upload_chunks(&mut chain_a, sequence, &packet_b_to_a, DUMMY_PROOF)
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
    let ack_b_data = extract_ack_data(&chain_b, recv_on_b.ack_pda).await;
    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &packet_a_to_b, DUMMY_PROOF)
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
    let ack_a_data = extract_ack_data(&chain_a, recv_on_a.ack_pda).await;
    let (b_ack_payload, b_ack_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, &packet_b_to_a, DUMMY_PROOF)
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
    let counter_b = read_user_counter(&chain_b, counter_on_b).await;
    assert_eq!(counter_b.count, amount_a_to_b);

    let counter_a = read_user_counter(&chain_a, counter_on_a).await;
    assert_eq!(counter_a.count, amount_b_to_a);

    assert_gmp_result_exists(&chain_a, chain_a.client_id(), sequence).await;
    assert_gmp_result_exists(&chain_b, chain_b.client_id(), sequence).await;
}
