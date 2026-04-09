use super::*;

/// Timeout lifecycle: send on A -> timeout on A (packet never delivered to B).
#[tokio::test]
async fn test_timeout_packet() {
    let user = User::new();
    let relayer = Relayer::new();
    let packet_data = b"this packet will time out";
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;

    // ── Build Chain A (only chain needed — timeout is delivered to source) ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });
    chain_a.prefund(&user);

    // ── Start chain ──
    chain_a.start().await;

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
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
            },
        )
        .await
        .expect("timeout_packet failed");

    assert_commitment_zeroed(&chain_a, commitment_pda).await;

    // Verify app state reflects the timeout
    let a_state = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
    assert_eq!(a_state.packets_sent, 1);
    assert_eq!(a_state.packets_timed_out, 1);
    assert_eq!(a_state.packets_acknowledged, 0);
}
