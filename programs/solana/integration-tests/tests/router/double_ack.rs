use super::*;

/// Acking the same packet twice fails — the commitment is already zeroed.
#[tokio::test]
async fn test_double_ack_fails() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let packet_data = b"double ack";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

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

    // Send → recv → ack (full lifecycle)
    user.send_packet(
        &mut chain_a,
        SendPacketParams {
            sequence,
            packet_data,
        },
    )
    .await
    .expect("send failed");

    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("upload recv chunks failed");
    relayer
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
        .expect("recv failed");

    let (a_payload, a_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload ack chunks failed");

    let ack_params = AckPacketParams {
        sequence,
        acknowledgement: successful_ack.clone(),
        payload_chunk_pda: a_payload,
        proof_chunk_pda: a_proof,
        app_program: test_ibc_app::ID,
        ..Default::default()
    };

    relayer
        .ack_packet(&mut chain_a, ack_params)
        .await
        .expect("first ack failed");

    // Cleanup consumed chunks, then re-upload for second attempt
    relayer
        .cleanup_chunks(&mut chain_a, sequence, a_payload, a_proof)
        .await
        .expect("cleanup chunks failed");
    let (a_payload, a_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("re-upload ack chunks failed");

    // Second ack — commitment is zeroed, should fail
    let err = relayer
        .ack_packet(
            &mut chain_a,
            AckPacketParams {
                sequence,
                acknowledgement: successful_ack,
                payload_chunk_pda: a_payload,
                proof_chunk_pda: a_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect_err("second ack should fail");

    assert_eq!(extract_custom_error(&err), PACKET_COMMITMENT_MISMATCH);
}
