use super::*;

#[tokio::test]
async fn test_gmp_full_lifecycle() {
    let user = User::new();
    let relayer = Relayer::new();
    let deployer = Deployer::new();
    let admin = Admin::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let increment_amount = 42u64;

    // ── Build Chain A (sender chain, with GMP) ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_a.prefund(&user);

    // ── Build Chain B (receiver chain, with GMP + test_gmp_app) ──
    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        deployer: &deployer,
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });

    // Derive GMP account PDA on Chain B and pre-fund it
    let gmp_account_pda = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    chain_b.prefund_lamports(gmp_account_pda, 10_000_000);

    // Derive target account PDAs on Chain B
    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_b.counter_app_state_pda();

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

    assert_commitment_set(&chain_a, commitment_pda).await;

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

    assert_receipt_created(&chain_b, recv.receipt_pda).await;

    let user_counter = read_user_counter(&chain_b, user_counter_pda).await;
    assert_eq!(
        user_counter.count, increment_amount,
        "UserCounter should have count == increment_amount"
    );

    let counter_state = read_counter_app_state(&chain_b, counter_app_state).await;
    assert_eq!(counter_state.total_counters, 1);

    // ──────────────────────────────────────────────────────────────────────
    // Relayer uploads chunks and delivers ack_packet back to Chain A
    // ──────────────────────────────────────────────────────────────────────
    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload ack chunks on Chain A failed");

    let ack_data = extract_ack_data(&chain_b, recv.ack_pda).await;

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

    assert_commitment_zeroed(&chain_a, ack_commitment_pda).await;
    assert_gmp_result_exists(&chain_a, chain_a.client_id(), sequence).await;
}
