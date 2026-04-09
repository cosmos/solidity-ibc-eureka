use super::*;

/// Error ack lifecycle: `mock_ibc_app` returns `b"error"` when payload starts
/// with `RETURN_ERROR_ACK`. The router stores the error ack commitment and the
/// full send -> recv -> ack flow completes successfully.
#[tokio::test]
async fn test_error_ack_lifecycle() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    // Payload prefix triggers error ack in mock_ibc_app (first 16 bytes checked)
    let packet_data = b"RETURN_ERROR_ACKextra";
    let error_ack = b"error".to_vec();

    // Chain A: test_ibc_app (sender)
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
    chain_a.prefund(&user);

    // Chain B: mock_ibc_app (receiver with magic-string ack control)
    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        deployer: &deployer,
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::MockIbcApp],
    });

    chain_a.start().await;
    chain_b.start().await;

    // User sends on A
    user.send_packet(
        &mut chain_a,
        SendPacketParams {
            sequence,
            packet_data,
        },
    )
    .await
    .expect("send_packet failed");

    // Relayer delivers to B — mock_ibc_app returns b"error"
    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("upload recv chunks failed");
    let recv = relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                proof_chunk_pda: b_proof,
                app_program: mock_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("recv_packet with error ack failed");

    // Verify ack was stored on B (non-zero commitment)
    let ack = chain_b
        .get_account(recv.ack_pda)
        .await
        .expect("ack should exist on B");
    assert_ne!(
        &ack.data[8..40],
        &[0u8; 32],
        "ack commitment should be non-zero"
    );

    // Verify the ack commitment matches hash of the error ack
    let expected_ack_commitment =
        ics24::packet_acknowledgement_commitment_bytes32(std::slice::from_ref(&error_ack))
            .expect("failed to compute ack commitment");
    assert_eq!(
        &ack.data[8..40],
        &expected_ack_commitment,
        "ack commitment should match hash of b\"error\""
    );

    // Relayer delivers ack back to A with the raw error ack bytes
    let (a_payload, a_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, packet_data, &proof_data)
        .await
        .expect("upload ack chunks failed");
    let commitment_pda = relayer
        .ack_packet(
            &mut chain_a,
            AckPacketParams {
                sequence,
                acknowledgement: error_ack,
                payload_chunk_pda: a_payload,
                proof_chunk_pda: a_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("ack_packet with error ack failed");

    assert_commitment_zeroed(&chain_a, commitment_pda).await;
}
