use super::*;

/// Light client rejects proof: `mock_light_client` returns an error when the
/// proof starts with `REJECT_PROOF`, causing the entire `recv_packet`
/// transaction to revert.
#[tokio::test]
async fn test_proof_verification_failure() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let packet_data = b"proof will be rejected";
    // Magic bytes that trigger mock_light_client rejection
    let bad_proof = b"REJECT_PROOF_bad_data".to_vec();
    let sequence = 1u64;

    // ── Chains ──
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let (mut chain_a, mut chain_b) = Chain::pair(&deployer, programs);
    chain_a.prefund(&[&admin, &relayer, &user]);
    chain_b.prefund(&[&admin, &relayer]);

    // ── Init ──
    chain_a.start().await;
    deployer
        .init_ibc_stack(&mut chain_a, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs)
        .await;
    chain_b.start().await;
    deployer
        .init_ibc_stack(&mut chain_b, &admin, &relayer, programs)
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

    // Relayer uploads chunks with the "bad" proof that mock_light_client rejects
    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &bad_proof)
        .await
        .expect("upload chunks failed");

    // recv_packet should fail — light client CPI error aborts the transaction
    let err = relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                proof_chunk_pda: b_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect_err("recv_packet with rejected proof should fail");

    // Verify it's a custom error (CPI failure propagates as custom error)
    let code = extract_custom_error(&err);
    assert_ne!(
        code, 0,
        "should have a non-zero error code from CPI failure"
    );
}
