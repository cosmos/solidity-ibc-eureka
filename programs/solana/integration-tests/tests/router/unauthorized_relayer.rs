use super::*;

/// `recv_packet` by a relayer without `RELAYER_ROLE` is rejected by the
/// access manager CPI during the router's `require_role` check.
#[tokio::test]
async fn test_unauthorized_relayer_rejected() {
    let user = User::new();
    let relayer = Relayer::new();
    let unauthorized = Relayer::new();
    let packet_data = b"unauthorized delivery";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;

    let deployer = Deployer::new();
    let admin = Admin::new();
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        programs,
    });
    chain_a.prefund(&[&admin, &relayer, &user]);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        deployer: &deployer,
        programs,
    });
    chain_b.prefund(&[&admin, &relayer, &unauthorized]);

    chain_a.start().await;
    deployer
        .init_programs(&mut chain_a, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs)
        .await;
    chain_b.start().await;
    deployer
        .init_programs(&mut chain_b, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_b, programs)
        .await;

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

    // Authorized relayer uploads chunks on B (upload requires RELAYER_ROLE)
    let (payload_pda, proof_pda) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("authorized relayer upload_chunks should succeed");

    // Unauthorized relayer attempts recv_packet — access manager rejects
    let err = unauthorized
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence,
                payload_chunk_pda: payload_pda,
                proof_chunk_pda: proof_pda,
                app_program: test_ibc_app::ID,
                ..Default::default()
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
