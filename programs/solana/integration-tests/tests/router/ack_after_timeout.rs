use super::*;

/// After a successful timeout, attempting to ack the same packet fails.
#[tokio::test]
async fn test_ack_after_timeout_fails() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let packet_data = b"timeout then ack";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

    // ── Chain ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        programs,
    });
    chain_a.prefund(&[&admin, &relayer, &user]);

    // ── Init ──
    chain_a.start().await;
    deployer
        .init_ibc_stack(&mut chain_a, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs)
        .await;

    user.send_packet(
        &mut chain_a,
        SendPacketParams {
            sequence,
            packet_data,
        },
    )
    .await
    .expect("send failed");

    // Timeout the packet
    let (payload_pda, proof_pda) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload timeout chunks failed");
    relayer
        .timeout_packet(
            &mut chain_a,
            TimeoutPacketParams {
                sequence,
                payload_chunk_pda: payload_pda,
                proof_chunk_pda: proof_pda,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("timeout failed");

    // Cleanup consumed timeout chunks, then upload fresh ones for the ack attempt
    relayer
        .cleanup_chunks(&mut chain_a, sequence, payload_pda, proof_pda)
        .await
        .expect("cleanup chunks failed");
    let (a_payload, a_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload ack chunks failed");

    // Now try to ack — commitment is zeroed, should fail
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
        .expect_err("ack after timeout should fail");

    assert_eq!(extract_custom_error(&err), PACKET_COMMITMENT_MISMATCH);
}
