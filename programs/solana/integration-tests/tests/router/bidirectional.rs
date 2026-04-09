use super::*;

/// Bidirectional: A->B and B->A with different sequences.
#[tokio::test]
async fn test_bidirectional_packets() {
    let user_a = User::new();
    let user_b = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

    let data_a_to_b = b"A says hello to B";
    let data_b_to_a = b"B says hello to A";
    let seq_a_to_b = 1u64;
    let seq_b_to_a = 2u64;

    // ── Build chains ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });
    chain_a.prefund(&user_a);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        deployer: &deployer,
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });
    chain_b.prefund(&user_b);

    // ── Start both chains ──
    chain_a.start().await;
    chain_b.start().await;

    // ── User A sends A→B ──
    user_a
        .send_packet(
            &mut chain_a,
            SendPacketParams {
                sequence: seq_a_to_b,
                packet_data: data_a_to_b,
            },
        )
        .await
        .expect("A->B send failed");

    // ── User B sends B→A ──
    user_b
        .send_packet(
            &mut chain_b,
            SendPacketParams {
                sequence: seq_b_to_a,
                packet_data: data_b_to_a,
            },
        )
        .await
        .expect("B->A send failed");

    // ── Relayer uploads chunks and delivers A→B to Chain B ──
    let (b_recv_payload, b_recv_proof) = relayer
        .upload_chunks(&mut chain_b, seq_a_to_b, data_a_to_b, &proof_data)
        .await
        .expect("upload B recv chunks failed");
    relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence: seq_a_to_b,
                payload_chunk_pda: b_recv_payload,
                proof_chunk_pda: b_recv_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("A->B recv on B failed");

    // ── Relayer uploads chunks and delivers B→A to Chain A ──
    let (a_recv_payload, a_recv_proof) = relayer
        .upload_chunks(&mut chain_a, seq_b_to_a, data_b_to_a, &proof_data)
        .await
        .expect("upload A recv chunks failed");
    relayer
        .recv_packet(
            &mut chain_a,
            RecvPacketParams {
                sequence: seq_b_to_a,
                payload_chunk_pda: a_recv_payload,
                proof_chunk_pda: a_recv_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("B->A recv on A failed");

    // ── Relayer uploads chunks and delivers A→B ack back to Chain A ──
    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, seq_a_to_b, data_a_to_b, &proof_data)
        .await
        .expect("upload A ack chunks failed");
    relayer
        .ack_packet(
            &mut chain_a,
            AckPacketParams {
                sequence: seq_a_to_b,
                acknowledgement: successful_ack.clone(),
                payload_chunk_pda: a_ack_payload,
                proof_chunk_pda: a_ack_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("A->B ack on A failed");

    // ── Relayer uploads chunks and delivers B→A ack back to Chain B ──
    let (b_ack_payload, b_ack_proof) = relayer
        .upload_chunks(&mut chain_b, seq_b_to_a, data_b_to_a, &proof_data)
        .await
        .expect("upload B ack chunks failed");
    relayer
        .ack_packet(
            &mut chain_b,
            AckPacketParams {
                sequence: seq_b_to_a,
                acknowledgement: successful_ack,
                payload_chunk_pda: b_ack_payload,
                proof_chunk_pda: b_ack_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("B->A ack on B failed");

    // ── Verify final counters ──
    let a_state = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
    assert_eq!(a_state.packets_sent, 1);
    assert_eq!(a_state.packets_received, 1);
    assert_eq!(a_state.packets_acknowledged, 1);

    let b_state = read_app_state(&chain_b, chain_b.accounts.app_state_pda).await;
    assert_eq!(b_state.packets_sent, 1);
    assert_eq!(b_state.packets_received, 1);
    assert_eq!(b_state.packets_acknowledged, 1);
}
