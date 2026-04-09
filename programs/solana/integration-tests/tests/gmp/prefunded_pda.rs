use super::*;

const EXTRA_PREFUND_LAMPORTS: u64 = 50_000_000;

/// Pre-existing lamports on the GMP account PDA do not break `init_if_needed`
/// or `invoke_signed` during the recv flow.
#[tokio::test]
async fn test_gmp_prefunded_pda_not_blocked() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let increment_amount = 42u64;

    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        programs: &[Program::Gmp],
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        programs: &[Program::Gmp],
    });

    let gmp_account_pda = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    // Pre-fund with significantly more lamports than the default
    chain_b.prefund_lamports(gmp_account_pda, EXTRA_PREFUND_LAMPORTS);

    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_b
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

    chain_a.start().await;
    chain_b.start().await;

    // ── Send ──
    user.send_call(
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

    // ── Recv ──
    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload recv chunks failed");

    let remaining = gmp::build_increment_remaining_accounts(
        gmp_account_pda,
        counter_app_state,
        user_counter_pda,
    );

    let recv = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                proof_chunk_pda: b_proof,
                remaining_accounts: remaining,
            },
        )
        .await
        .expect("recv_packet should succeed despite pre-funded PDA");

    // Counter was incremented
    let user_counter_account = chain_b
        .get_account(user_counter_pda)
        .await
        .expect("UserCounter should exist");
    let user_counter =
        test_gmp_app::state::UserCounter::try_deserialize(&mut &user_counter_account.data[..])
            .expect("deserialize UserCounter");
    assert_eq!(user_counter.count, increment_amount);

    // ── Ack ──
    let ack_data = chain_b
        .get_account(recv.ack_pda)
        .await
        .expect("ack should exist")
        .data[8..40]
        .to_vec();

    let (a_payload, a_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload ack chunks failed");

    let ack_commitment_pda = relayer
        .gmp_ack_packet(
            &mut chain_a,
            GmpAckPacketParams {
                sequence,
                acknowledgement: ack_data,
                payload_chunk_pda: a_payload,
                proof_chunk_pda: a_proof,
            },
        )
        .await
        .expect("ack_packet failed");

    // Commitment zeroed
    let commitment = chain_a
        .get_account(ack_commitment_pda)
        .await
        .expect("commitment should exist");
    assert_eq!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be zeroed after ack"
    );

    // GMPCallResultAccount exists
    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(chain_a.client_id(), sequence, &ics27_gmp::ID);
    assert!(chain_a.get_account(result_pda).await.is_some());
}
