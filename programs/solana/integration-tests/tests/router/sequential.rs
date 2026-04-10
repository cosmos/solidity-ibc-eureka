use super::*;

/// Send 3 packets A->B, recv all on B, ack all on A.
#[tokio::test]
async fn test_multiple_sequential_packets() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let proof_data = vec![0u8; 32];
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();
    let packets: [(u64, &[u8]); 3] = [(1, b"packet one"), (2, b"packet two"), (3, b"packet three")];

    // ── Chains ──
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

    // ── User sends all 3 packets on A ──
    for &(seq, data) in &packets {
        user.send_packet(
            &mut chain_a,
            SendPacketParams {
                sequence: seq,
                packet_data: data,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("send seq={seq} failed: {e:?}"));
    }

    let a_state = read_app_state(&chain_a).await;
    assert_eq!(a_state.packets_sent, 3);

    // ── Relayer uploads chunks and delivers all 3 packets to B ──
    for &(seq, data) in &packets {
        let (payload, proof) = relayer
            .upload_chunks(&mut chain_b, seq, data, &proof_data)
            .await
            .unwrap_or_else(|e| panic!("upload B recv chunks seq={seq} failed: {e:?}"));
        relayer
            .recv_packet(
                &mut chain_b,
                RecvPacketParams {
                    sequence: seq,
                    payload_chunk_pda: payload,
                    proof_chunk_pda: proof,
                    app_program: test_ibc_app::ID,
                    ..Default::default()
                },
            )
            .await
            .unwrap_or_else(|e| panic!("recv seq={seq} failed: {e:?}"));
    }

    let b_state = read_app_state(&chain_b).await;
    assert_eq!(b_state.packets_received, 3);

    // ── Relayer uploads chunks and delivers all 3 acks back to A ──
    for &(seq, data) in &packets {
        let (payload, proof) = relayer
            .upload_chunks(&mut chain_a, seq, data, &proof_data)
            .await
            .unwrap_or_else(|e| panic!("upload A ack chunks seq={seq} failed: {e:?}"));
        let commitment_pda = relayer
            .ack_packet(
                &mut chain_a,
                AckPacketParams {
                    sequence: seq,
                    acknowledgement: successful_ack.clone(),
                    payload_chunk_pda: payload,
                    proof_chunk_pda: proof,
                    app_program: test_ibc_app::ID,
                    ..Default::default()
                },
            )
            .await
            .unwrap_or_else(|e| panic!("ack seq={seq} failed: {e:?}"));

        assert_commitment_zeroed(&chain_a, commitment_pda).await;
    }

    // ── Verify final counters ──
    let a_state = read_app_state(&chain_a).await;
    assert_eq!(a_state.packets_sent, 3);
    assert_eq!(a_state.packets_acknowledged, 3);

    let b_state = read_app_state(&chain_b).await;
    assert_eq!(b_state.packets_received, 3);
}
