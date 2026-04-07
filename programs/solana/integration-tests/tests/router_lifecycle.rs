//! Solana-to-Solana IBC router integration tests.
//!
//! Two independent chains run as separate `ProgramTest` instances. The
//! `Relayer` actor delivers packets between them while the `User` actor
//! initiates sends.
//!
//! The mock light client always accepts proofs, so these tests exercise the
//! full IBC router lifecycle (send -> recv -> ack) without real proof
//! verification.

use anchor_lang::AccountDeserialize;
use integration_tests::{
    chain::{Chain, ChainConfig, TEST_CLOCK_TIME},
    relayer::Relayer,
    router::{self, AckPacketParams, RecvPacketParams, SendPacketParams},
    user::User,
};
use solana_ibc_types::ics24;
use solana_sdk::pubkey::Pubkey;

async fn read_app_state(
    chain: &Chain,
    app_state_pda: Pubkey,
) -> test_ibc_app::state::TestIbcAppState {
    let account = chain
        .get_account(app_state_pda)
        .await
        .expect("app state should exist");
    test_ibc_app::state::TestIbcAppState::try_deserialize(&mut &account.data[..])
        .expect("failed to deserialize app state")
}

/// Full lifecycle: send on A -> recv on B -> ack on A.
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
        include_gmp: false,
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: false,
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
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: false,
    });
    chain_a.prefund(&user_a);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: false,
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
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
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
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
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
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
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
                port_id: router::PORT_ID,
                version: "1",
                encoding: "json",
                app_program: test_ibc_app::ID,
                extra_remaining_accounts: vec![],
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

/// Send 3 packets A->B, recv all on B, ack all on A.
#[tokio::test]
async fn test_multiple_sequential_packets() {
    let user = User::new();
    let relayer = Relayer::new();
    let proof_data = vec![0u8; 32];
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

    let packets: [(u64, &[u8]); 3] = [
        (1, b"packet one"),
        (2, b"packet two"),
        (3, b"packet three"),
    ];

    // ── Build chains ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: false,
    });
    chain_a.prefund(&user);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        clock_time: TEST_CLOCK_TIME,
        include_gmp: false,
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

/// Timeout lifecycle: send on A -> timeout on A (packet never delivered to B).
#[tokio::test]
async fn test_timeout_packet() {
    use integration_tests::router::TimeoutPacketParams;

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
        clock_time: TEST_CLOCK_TIME,
        include_gmp: false,
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

    // Verify commitment was created
    let commitment = chain_a
        .get_account(send.commitment_pda)
        .await
        .expect("commitment should exist");
    assert_ne!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be non-zero after send"
    );

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

    // Verify commitment was zeroed
    let commitment = chain_a
        .get_account(commitment_pda)
        .await
        .expect("commitment PDA should still exist");
    assert_eq!(
        &commitment.data[8..40],
        &[0u8; 32],
        "commitment should be zeroed after timeout"
    );

    // Verify app state reflects the timeout
    let a_state = read_app_state(&chain_a, chain_a.accounts.app_state_pda).await;
    assert_eq!(a_state.packets_sent, 1);
    assert_eq!(a_state.packets_timed_out, 1);
    assert_eq!(a_state.packets_acknowledged, 0);
}
