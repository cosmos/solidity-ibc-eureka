use super::*;

/// Light client rejects proof: `mock_light_client` returns an error when the
/// proof starts with `REJECT_PROOF`, causing the entire `recv_packet`
/// transaction to revert.
#[tokio::test]
async fn test_proof_verification_failure() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"proof will be rejected";
    // Magic bytes that trigger mock_light_client rejection
    let bad_proof = b"REJECT_PROOF_bad_data".to_vec();
    let sequence = 1u64;

    let admin = Admin::new();
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
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

    // Relayer uploads chunks with the "bad" proof that mock_light_client rejects
    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, packet_data, &bad_proof)
        .await
        .expect("upload chunks failed");

    // recv_packet should fail — light client CPI error aborts the transaction
    let err = relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                proof_chunk_pda: b_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect_err("recv_packet with rejected proof should fail");

    // Verify it's a custom error (CPI failure propagates as custom error)
    let code = extract_custom_error(&err);
    assert_ne!(
        code, 0,
        "should have a non-zero error code from CPI failure"
    );
}
