use super::*;

/// Receiving the same packet twice is a noop — the app callback is NOT
/// invoked again and the `packets_received` counter stays at 1.
#[tokio::test]
async fn test_recv_packet_replay_is_noop() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let packet_data = b"replay me";
    let proof_data = vec![0u8; 32];
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

    // Send on A
    user.send_packet(
        &mut chain_a,
        SendPacketParams {
            sequence,
            packet_data,
        },
    )
    .await
    .expect("send_packet failed");

    // First recv on B
    let (payload_pda, proof_pda) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("upload chunks failed");

    relayer
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
        .expect("first recv_packet failed");

    let b_state = read_app_state(&chain_b).await;
    assert_eq!(b_state.packets_received, 1);

    // Cleanup consumed chunks (relayer reclaims rent, accounts fully closed)
    relayer
        .cleanup_chunks(&mut chain_b, sequence, payload_pda, proof_pda)
        .await
        .expect("cleanup chunks failed");

    // Re-upload fresh chunks for the second attempt
    let (payload_pda, proof_pda) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("re-upload chunks failed");

    // Second recv on B — noop, no error
    relayer
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
        .expect("second recv_packet should succeed (noop)");

    let b_state = read_app_state(&chain_b).await;
    assert_eq!(
        b_state.packets_received, 1,
        "packets_received should not increment on replay"
    );
}
