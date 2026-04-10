use super::*;

#[tokio::test]
async fn test_full_packet_lifecycle() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let packet_data = b"hello from chain A";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

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

    let a_state = read_app_state(&chain_a).await;
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
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("recv_packet on B failed");

    assert_receipt_created(&chain_b, recv.receipt_pda).await;
    assert_commitment_set(&chain_b, recv.ack_pda).await;

    let b_state = read_app_state(&chain_b).await;
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
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("ack_packet on A failed");

    assert_commitment_zeroed(&chain_a, commitment_pda).await;

    let a_final = read_app_state(&chain_a).await;
    assert_eq!(a_final.packets_sent, 1);
    assert_eq!(a_final.packets_acknowledged, 1);
}
