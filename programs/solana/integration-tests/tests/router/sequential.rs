use super::*;

/// Send 3 packets A->B, recv all on B, ack all on A.
#[tokio::test]
async fn test_multiple_sequential_packets() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

    let packets: [(u64, &[u8]); 3] = [(1, b"packet one"), (2, b"packet two"), (3, b"packet three")];

    // ── Build chains ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        ibc_app: IbcApp::TestIbcApp,
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        ibc_app: IbcApp::TestIbcApp,
    });

    // ── Start both chains ──
    chain_a.start().await;
    chain_b.start().await;

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

    let a_state = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
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
                    port_id: router::PORT_ID,
                    version: "1",
                    encoding: "json",
                    app_program: test_ibc_app::ID,
                    extra_remaining_accounts: vec![],
                },
            )
            .await
            .unwrap_or_else(|e| panic!("recv seq={seq} failed: {e:?}"));
    }

    let b_state = read_app_state(&chain_b, chain_b.accounts.app_state_pda).await;
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
                    port_id: router::PORT_ID,
                    version: "1",
                    encoding: "json",
                    app_program: test_ibc_app::ID,
                    extra_remaining_accounts: vec![],
                },
            )
            .await
            .unwrap_or_else(|e| panic!("ack seq={seq} failed: {e:?}"));

        // Verify commitment zeroed
        let account = chain_a
            .get_account(commitment_pda)
            .await
            .expect("commitment should exist");
        assert_eq!(
            &account.data[8..40],
            &[0u8; 32],
            "commitment for seq={seq} should be zeroed"
        );
    }

    // ── Verify final counters ──
    let a_state = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
    assert_eq!(a_state.packets_sent, 3);
    assert_eq!(a_state.packets_acknowledged, 3);

    let b_state = read_app_state(&chain_b, chain_b.accounts.app_state_pda).await;
    assert_eq!(b_state.packets_received, 3);
}
