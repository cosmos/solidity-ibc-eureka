use super::*;

/// Empty ack rejection: `mock_ibc_app` returns `vec![]` when payload starts
/// with `RETURN_EMPTY_ACK`. The router rejects empty acks with
/// `AsyncAcknowledgementNotSupported`.
#[tokio::test]
async fn test_empty_ack_rejected() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    // Payload prefix triggers empty ack in mock_ibc_app
    let packet_data = b"RETURN_EMPTY_ACKextra";

    // Chain A: test_ibc_app (sender)
    let deployer = Deployer::new();
    let admin = Admin::new();
    let programs_a: &[&dyn ChainProgram] = &[&TestIbcApp];
    let programs_b: &[&dyn ChainProgram] = &[&MockIbcApp];
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        programs: programs_a,
    });
    chain_a.prefund(&[&admin, &relayer, &user]);

    // Chain B: mock_ibc_app (receiver)
    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        deployer: &deployer,
        programs: programs_b,
    });
    chain_b.prefund(&[&admin, &relayer]);

    chain_a.start().await;
    deployer
        .init_programs(&mut chain_a, &admin, &relayer, programs_a)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs_a)
        .await;
    chain_b.start().await;
    deployer
        .init_programs(&mut chain_b, &admin, &relayer, programs_b)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_b, programs_b)
        .await;

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

    // Relayer delivers to B — mock_ibc_app returns empty ack, router rejects
    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &proof_data)
        .await
        .expect("upload recv chunks failed");
    let err = relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                proof_chunk_pda: b_proof,
                app_program: mock_ibc_app::ID,
                app_state_pda: mock_ibc_app_state_pda(),
                ..Default::default()
            },
        )
        .await
        .expect_err("recv_packet with empty ack should fail");

    assert_eq!(
        extract_custom_error(&err),
        ASYNC_ACK_NOT_SUPPORTED,
        "should fail with AsyncAcknowledgementNotSupported"
    );
}
