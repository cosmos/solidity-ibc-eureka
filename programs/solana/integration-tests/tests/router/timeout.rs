use super::*;

/// Timeout lifecycle: send on A -> timeout on A (packet never delivered to B).
#[tokio::test]
async fn test_timeout_packet() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"this packet will time out";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;

    // ── Build Chain A (only chain needed — timeout is delivered to source) ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        ibc_app: IbcApp::TestIbcApp,
    });
    chain_a.prefund(&user);

    // ── Start chain ──
    chain_a.start().await;

    // ── User sends packet on Chain A ──
    let send = user
        .send_packet(
            &mut chain_a,
            SendPacketParams {
                sequence,
                packet_data,
            },
        )
        .await
        .expect("send_packet failed");

    // Verify commitment was created
    let commitment = chain_a
        .get_account(send.commitment_pda)
        .await
        .expect("commitment should exist");
    assert_ne!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be non-zero after send"
    );

    // ── Relayer uploads chunks and delivers timeout on Chain A ──
    let (timeout_payload, timeout_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload timeout chunks failed");
    let commitment_pda = relayer
        .timeout_packet(
            &mut chain_a,
            TimeoutPacketParams {
                sequence,
                payload_chunk_pda: timeout_payload,
                proof_chunk_pda: timeout_proof,
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
            },
        )
        .await
        .expect("timeout_packet failed");

    // Verify commitment was zeroed
    let commitment = chain_a
        .get_account(commitment_pda)
        .await
        .expect("commitment PDA should still exist");
    assert_eq!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be zeroed after timeout"
    );

    // Verify app state reflects the timeout
    let a_state = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
    assert_eq!(a_state.packets_sent, 1);
    assert_eq!(a_state.packets_timed_out, 1);
    assert_eq!(a_state.packets_acknowledged, 0);
}
