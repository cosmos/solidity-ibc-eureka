use super::*;

#[tokio::test]
async fn test_full_packet_lifecycle() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"hello from chain A";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

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

    // ── User sends on Chain A ──
    let send = user
        .send_packet(
            &mut chain_a,
            SendPacketParams {
                sequence,
                packet_data,
            },
        )
        .await
        .expect("send_packet on A failed");

    // Verify commitment
    let commitment_account = chain_a
        .get_account(send.commitment_pda)
        .await
        .expect("commitment should exist on chain A");
    assert_eq!(commitment_account.owner, ics26_router::ID);
    let expected_commitment = ics24::packet_commitment_bytes32(&send.packet);
    assert_eq!(&commitment_account.data[8..40], &expected_commitment);

    let a_state = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
    assert_eq!(a_state.packets_sent, 1);

    // ── Relayer uploads chunks and delivers to Chain B ──
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
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
            },
        )
        .await
        .expect("recv_packet on B failed");

    // Verify receipt and ack on B
    let receipt = chain_b
        .get_account(recv.receipt_pda)
        .await
        .expect("receipt should exist");
    assert_eq!(receipt.owner, ics26_router::ID);
    assert_ne!(&receipt.data[8..40], &[0u8; 32]);

    let ack = chain_b
        .get_account(recv.ack_pda)
        .await
        .expect("ack should exist");
    assert_eq!(ack.owner, ics26_router::ID);
    assert_ne!(&ack.data[8..40], &[0u8; 32]);

    let b_state = read_app_state(&chain_b, chain_b.accounts.app_state_pda).await;
    assert_eq!(b_state.packets_received, 1);

    // ── Relayer uploads chunks and delivers ack back to Chain A ──
    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload ack chunks on A failed");
    let commitment_pda = relayer
        .ack_packet(
            &mut chain_a,
            AckPacketParams {
                sequence,
                acknowledgement: successful_ack,
                payload_chunk_pda: a_ack_payload,
                proof_chunk_pda: a_ack_proof,
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
            },
        )
        .await
        .expect("ack_packet on A failed");

    // Verify commitment zeroed
    let commitment = chain_a
        .get_account(commitment_pda)
        .await
        .expect("commitment PDA should still exist");
    assert_eq!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be zeroed after ack"
    );

    let a_final = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
    assert_eq!(a_final.packets_sent, 1);
    assert_eq!(a_final.packets_acknowledged, 1);
}
