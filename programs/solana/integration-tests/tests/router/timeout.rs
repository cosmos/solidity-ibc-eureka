use super::*;

/// Timeout lifecycle: send on A -> timeout on A (packet never delivered to B).
#[tokio::test]
async fn test_timeout_packet() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let packet_data = b"this packet will time out";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;

    // ── Chain ──
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let mut chain_a = Chain::single(&deployer, programs);
    chain_a.prefund(&[&admin, &relayer, &user]);

    // ── Init ──
    chain_a.start().await;
    deployer
        .init_ibc_stack(&mut chain_a, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs)
        .await;

    // ── User sends packet on Chain A ──
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

    assert_commitment_set(&chain_a, send.commitment_pda).await;

    // ── Relayer uploads chunks and delivers timeout on Chain A ──
    let (timeout_payload, timeout_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload timeout chunks failed");
    let commitment_pda = relayer
        .timeout_packet(
            &mut chain_a,
            TimeoutPacketParams {
                sequence,
                payload_chunk_pda: timeout_payload,
                proof_chunk_pda: timeout_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("timeout_packet failed");

    assert_commitment_zeroed(&chain_a, commitment_pda).await;

    // Verify app state reflects the timeout
    let a_state = read_app_state(&chain_a).await;
    assert_eq!(a_state.packets_sent, 1);
    assert_eq!(a_state.packets_timed_out, 1);
    assert_eq!(a_state.packets_acknowledged, 0);
}
