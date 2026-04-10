use super::*;

/// Source chain times out a packet, but the destination chain independently
/// accepts `recv_packet` (chains don't share state). The subsequent `ack_packet`
/// back on the source fails because the commitment is already zeroed.
#[tokio::test]
async fn test_recv_after_source_timeout() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"timeout then recv";
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
    chain_b.prefund(&[&admin, &relayer]);

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

    // ── Step 1: User sends on A ──
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

    // ── Step 2: Relayer times out on A ──
    let (a_to_payload, a_to_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload timeout chunks failed");

    relayer
        .timeout_packet(
            &mut chain_a,
            TimeoutPacketParams {
                sequence,
                payload_chunk_pda: a_to_payload,
                proof_chunk_pda: a_to_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("timeout_packet on source failed");

    assert_commitment_zeroed(&chain_a, send.commitment_pda).await;

    // ── Step 3: Relayer delivers recv on B (succeeds — B is independent) ──
    let (b_recv_payload, b_recv_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("upload recv chunks on B failed");

    let recv = relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence,
                payload_chunk_pda: b_recv_payload,
                proof_chunk_pda: b_recv_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("recv_packet on dest should succeed despite source timeout");

    assert_receipt_created(&chain_b, recv.receipt_pda).await;
    let ack_data = extract_ack_data(&chain_b, recv.ack_pda).await;

    // ── Step 4: Relayer attempts ack on A — fails (commitment already zeroed) ──
    // Cleanup timeout chunks first so the same PDAs can be re-created for ack
    relayer
        .cleanup_chunks(&mut chain_a, sequence, a_to_payload, a_to_proof)
        .await
        .expect("cleanup timeout chunks failed");

    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload ack chunks on A failed");

    let err = relayer
        .ack_packet(
            &mut chain_a,
            AckPacketParams {
                sequence,
                acknowledgement: ack_data,
                payload_chunk_pda: a_ack_payload,
                proof_chunk_pda: a_ack_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect_err("ack_packet should fail — commitment already zeroed by timeout");

    assert_eq!(
        extract_custom_error(&err),
        PACKET_COMMITMENT_MISMATCH,
        "expected PACKET_COMMITMENT_MISMATCH"
    );
}
