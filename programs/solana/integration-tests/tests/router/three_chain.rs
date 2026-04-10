use super::*;

/// Three-chain plain-router roundtrip: A→B then B→C, with independent IBC
/// packet lifecycles on each hop.
///
/// Mirrors `tests/gmp/three_chain.rs` but exercises the bare router via
/// `test_ibc_app` instead of the GMP application stack. Verifies that a chain
/// in the middle of a multi-hop path can both receive on its primary client
/// and dispatch a fresh send via a secondary client.
///
/// Chain A: client `"a-to-b"` ↔ counterparty `"b-to-a"`
/// Chain B: primary `"b-to-a"` ↔ `"a-to-b"`, additional `"b-to-c"` ↔ `"c-to-b"`
/// Chain C: client `"c-to-b"` ↔ counterparty `"b-to-c"`
#[tokio::test]
async fn test_router_three_chain_roundtrip() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let packet_data_ab = b"hello from chain A";
    let packet_data_bc = b"hello from chain B";
    let successful_ack = br#"{"result": "AQ=="}"#.to_vec();

    // ── Chains ──
    let programs: &[&dyn ChainProgram] = &[&TestIbcApp];
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "a-to-b",
        counterparty_client_id: "b-to-a",
        deployer: &deployer,
        programs,
    });
    chain_a.prefund(&[&admin, &relayer, &user]);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "b-to-a",
        counterparty_client_id: "a-to-b",
        deployer: &deployer,
        programs,
    });
    chain_b.prefund(&[&admin, &relayer, &user]);

    let mut chain_c = Chain::new(ChainConfig {
        client_id: "c-to-b",
        counterparty_client_id: "b-to-c",
        deployer: &deployer,
        programs,
    });
    chain_c.prefund(&[&admin, &relayer]);

    // ── Init ──
    chain_a.init(&deployer, &admin, &relayer, programs).await;

    // Chain B needs the standard init plus a second client (`b-to-c`) wired
    // up via `add_counterparty` before we hand off upgrade authority.
    chain_b.start().await;
    deployer
        .init_ibc_stack(&mut chain_b, &admin, &relayer, programs)
        .await;
    deployer
        .add_counterparty(&mut chain_b, &admin, "b-to-c", "c-to-b")
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_b, programs)
        .await;

    chain_c.init(&deployer, &admin, &relayer, programs).await;

    // ── Leg 1: A → B ──
    let send_a = user
        .send_packet(
            &mut chain_a,
            SendPacketParams {
                sequence: 1,
                packet_data: packet_data_ab,
            },
        )
        .await
        .expect("A→B send_packet failed");
    assert_commitment_set(&chain_a, send_a.commitment_pda).await;

    let (b_recv_payload, b_recv_proof) = relayer
        .upload_chunks(&mut chain_b, 1, packet_data_ab, DUMMY_PROOF)
        .await
        .expect("upload A→B recv chunks failed");
    let recv_on_b = relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence: 1,
                payload_chunk_pda: b_recv_payload,
                proof_chunk_pda: b_recv_proof,
                ..Default::default()
            },
        )
        .await
        .expect("A→B recv_packet failed");
    assert_receipt_created(&chain_b, recv_on_b.receipt_pda).await;
    assert_commitment_set(&chain_b, recv_on_b.ack_pda).await;

    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, 1, packet_data_ab, DUMMY_PROOF)
        .await
        .expect("upload A→B ack chunks failed");
    let ack_commitment_a = relayer
        .ack_packet(
            &mut chain_a,
            AckPacketParams {
                sequence: 1,
                acknowledgement: successful_ack.clone(),
                payload_chunk_pda: a_ack_payload,
                proof_chunk_pda: a_ack_proof,
                ..Default::default()
            },
        )
        .await
        .expect("A→B ack_packet failed");
    assert_commitment_zeroed(&chain_a, ack_commitment_a).await;

    // ── Leg 2: B → C ──
    // Send via the secondary `b-to-c` client on chain B (the relayer/user
    // helpers default to the chain's primary `client_id`, so we build the
    // instruction directly).
    let send_bc = router::build_send_packet_ix(
        user.pubkey(),
        "b-to-c",
        "c-to-b",
        chain_b.clock_time(),
        SendPacketParams {
            sequence: 1,
            packet_data: packet_data_bc,
        },
    );
    let tx = Transaction::new_signed_with_payer(
        std::slice::from_ref(&send_bc.ix),
        Some(&user.pubkey()),
        &[user.keypair()],
        chain_b.blockhash(),
    );
    chain_b
        .process_transaction(tx)
        .await
        .expect("B→C send_packet failed");
    assert_commitment_set(&chain_b, send_bc.commitment_pda).await;

    let (c_recv_payload, c_recv_proof) = relayer
        .upload_chunks(&mut chain_c, 1, packet_data_bc, DUMMY_PROOF)
        .await
        .expect("upload B→C recv chunks failed");
    let recv_on_c = relayer
        .recv_packet(
            &mut chain_c,
            RecvPacketParams {
                sequence: 1,
                payload_chunk_pda: c_recv_payload,
                proof_chunk_pda: c_recv_proof,
                ..Default::default()
            },
        )
        .await
        .expect("B→C recv_packet failed");
    assert_receipt_created(&chain_c, recv_on_c.receipt_pda).await;
    assert_commitment_set(&chain_c, recv_on_c.ack_pda).await;

    // Ack back C → B on the secondary client.
    let (b_ack_payload, b_ack_proof) = relayer
        .upload_chunks_for_client(&mut chain_b, "b-to-c", 1, packet_data_bc, DUMMY_PROOF)
        .await
        .expect("upload B→C ack chunks failed");
    let (ack_bc_ix, ack_commitment_b) = router::build_ack_packet_ix(
        relayer.pubkey(),
        "b-to-c",
        "c-to-b",
        chain_b.clock_time(),
        AckPacketParams {
            sequence: 1,
            acknowledgement: successful_ack,
            payload_chunk_pda: b_ack_payload,
            proof_chunk_pda: b_ack_proof,
            ..Default::default()
        },
    );
    assert_eq!(ack_commitment_b, send_bc.commitment_pda);
    let tx = Transaction::new_signed_with_payer(
        &[ack_bc_ix],
        Some(&relayer.pubkey()),
        &[relayer.keypair()],
        chain_b.blockhash(),
    );
    chain_b
        .process_transaction(tx)
        .await
        .expect("B→C ack_packet failed");
    assert_commitment_zeroed(&chain_b, ack_commitment_b).await;

    // ── Final assertions ──
    // Chain A: 1 sent, 1 acked.
    let a_state = read_app_state(&chain_a).await;
    assert_eq!(a_state.packets_sent, 1);
    assert_eq!(a_state.packets_acknowledged, 1);

    // Chain B: 1 received (leg 1) + 1 sent + 1 acked (leg 2).
    let b_state = read_app_state(&chain_b).await;
    assert_eq!(b_state.packets_sent, 1);
    assert_eq!(b_state.packets_received, 1);
    assert_eq!(b_state.packets_acknowledged, 1);

    // Chain C: 1 received (leg 2).
    let c_state = read_app_state(&chain_c).await;
    assert_eq!(c_state.packets_received, 1);
}
