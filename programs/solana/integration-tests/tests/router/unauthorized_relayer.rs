use super::*;

/// A relayer without `RELAYER_ROLE` can upload chunks (no role check) but
/// `recv_packet` is rejected by the access manager CPI during the router's
/// `require_role` check.
#[tokio::test]
async fn test_unauthorized_relayer_rejected() {
    let user = User::new();
    let relayer = Relayer::new();
    let unauthorized = Relayer::new();
    let packet_data = b"unauthorized delivery";
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

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        ibc_app: IbcApp::TestIbcApp,
    });
    chain_b.prefund(&unauthorized);

    chain_a.start().await;
    chain_b.start().await;

    // User sends on A
    user.send_packet(
        &mut chain_a,
        SendPacketParams {
            sequence,
            packet_data,
        },
    )
    .await
    .expect("send_packet failed");

    // Unauthorized relayer uploads chunks on B (no role check — succeeds)
    let (payload_pda, proof_pda) = unauthorized
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("upload_chunks should succeed without RELAYER_ROLE");

    // Unauthorized relayer attempts recv_packet — access manager rejects
    let err = unauthorized
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
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
        .expect_err("recv_packet should fail for unauthorized relayer");

    let code = extract_custom_error(&err);
    assert_ne!(
        code, 0,
        "should have a non-zero error code from access manager rejection"
    );

    // Verify no receipt was created (recv_packet reverted)
    let receipt_pda = router::derive_receipt_pda(chain_b.client_id(), sequence);
    assert!(
        chain_b.get_account(receipt_pda).await.is_none(),
        "no receipt should exist after rejected recv_packet"
    );
}
