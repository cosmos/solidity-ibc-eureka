use super::*;

/// Full lifecycle with a 2-chunk proof: the proof exceeds `CHUNK_DATA_SIZE`
/// (900 bytes) and is split across two chunk accounts.
#[tokio::test]
async fn test_multi_chunk_proof_lifecycle() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"multi-chunk proof test";
    let sequence = 1u64;
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

    // Proof > 900 bytes: needs 2 chunks (900 + 300)
    let proof_data = vec![0xAB; 1200];
    let proof_chunk_0 = proof_data[..900].to_vec();
    let proof_chunk_1 = proof_data[900..].to_vec();

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

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        deployer: &deployer,
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

    // Relayer uploads 1 payload chunk + 2 proof chunks to B
    let (b_payload, b_proof_pdas) = relayer
        .upload_chunks_with_multi_proof(
            &mut chain_b,
            sequence,
            packet_data,
            &[proof_chunk_0.clone(), proof_chunk_1.clone()],
        )
        .await
        .expect("upload multi-chunk proof failed on B");

    // Relayer delivers recv_packet with 2 proof chunks
    let recv = relayer
        .recv_packet_multi_proof(
            &mut chain_b,
            RecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
            &b_proof_pdas,
        )
        .await
        .expect("recv_packet with multi-chunk proof failed");

    assert_receipt_created(&chain_b, recv.receipt_pda).await;
    assert_commitment_set(&chain_b, recv.ack_pda).await;

    // Relayer uploads 1 payload chunk + 2 proof chunks to A for ack
    let (a_payload, a_proof_pdas) = relayer
        .upload_chunks_with_multi_proof(
            &mut chain_a,
            sequence,
            packet_data,
            &[proof_chunk_0, proof_chunk_1],
        )
        .await
        .expect("upload multi-chunk proof failed on A");

    // Relayer delivers ack_packet with 2 proof chunks
    let commitment_pda = relayer
        .ack_packet_multi_proof(
            &mut chain_a,
            AckPacketParams {
                sequence,
                acknowledgement: successful_ack,
                payload_chunk_pda: a_payload,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
            &a_proof_pdas,
        )
        .await
        .expect("ack_packet with multi-chunk proof failed");

    assert_commitment_zeroed(&chain_a, commitment_pda).await;
}
