use super::*;

/// Receiving the same packet twice is a noop — the app callback is NOT
/// invoked again and the `packets_received` counter stays at 1.
#[tokio::test]
async fn test_recv_packet_replay_is_noop() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"replay me";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;

    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });

    chain_a.start().await;
    chain_b.start().await;

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

    let b_state = read_app_state(&chain_b, chain_b.accounts.app_state_pda).await;
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

    let b_state = read_app_state(&chain_b, chain_b.accounts.app_state_pda).await;
    assert_eq!(
        b_state.packets_received, 1,
        "packets_received should not increment on replay"
    );
}
