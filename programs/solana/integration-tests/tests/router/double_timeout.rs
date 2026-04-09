use super::*;

/// Timing out the same packet twice fails — the commitment is already zeroed.
#[tokio::test]
async fn test_double_timeout_fails() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"double timeout";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;

    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        ibc_app: IbcApp::TestIbcApp,
    });
    chain_a.prefund(&user);

    chain_a.start().await;

    user.send_packet(
        &mut chain_a,
        SendPacketParams {
            sequence,
            packet_data,
        },
    )
    .await
    .expect("send failed");

    let (payload_pda, proof_pda) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload timeout chunks failed");

    let timeout_params = TimeoutPacketParams {
        sequence,
        payload_chunk_pda: payload_pda,
        proof_chunk_pda: proof_pda,
        port_id: router::PORT_ID,
        version: "1",
        encoding: "json",
        app_program: test_ibc_app::ID,
        extra_remaining_accounts: vec![],
    };

    relayer
        .timeout_packet(&mut chain_a, timeout_params)
        .await
        .expect("first timeout failed");

    // Cleanup consumed chunks, then re-upload for second attempt
    relayer
        .cleanup_chunks(&mut chain_a, sequence, payload_pda, proof_pda)
        .await
        .expect("cleanup chunks failed");
    let (payload_pda, proof_pda) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("re-upload timeout chunks failed");

    // Second timeout — commitment is zeroed, should fail
    let err = relayer
        .timeout_packet(
            &mut chain_a,
            TimeoutPacketParams {
                sequence,
                payload_chunk_pda: payload_pda,
                proof_chunk_pda: proof_pda,
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
            },
        )
        .await
        .expect_err("second timeout should fail");

    assert_eq!(extract_custom_error(&err), PACKET_COMMITMENT_MISMATCH);
}
