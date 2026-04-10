use super::*;

/// `recv_packet` by a relayer without `RELAYER_ROLE` is rejected by the
/// access manager CPI during the router's `require_role` check.
#[tokio::test]
async fn test_unauthorized_relayer_rejected() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();
    let unauthorized = Relayer::new();

    // ── Test data ──
    let packet_data = b"unauthorized delivery";
    let sequence = 1u64;

    // ── Chains ──
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let (mut chain_a, mut chain_b) = Chain::pair(&deployer, programs);
    chain_a.prefund(&[&admin, &relayer, &user]);
    chain_b.prefund(&[&admin, &relayer, &unauthorized]);

    // ── Init ──
    chain_a.init(&deployer, &admin, &relayer, programs).await;
    chain_b.init(&deployer, &admin, &relayer, programs).await;

    // ── User sends on A ──
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
        .upload_chunks(&mut chain_b, sequence, packet_data, DUMMY_PROOF)
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
