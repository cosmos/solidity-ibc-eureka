use super::*;
use integration_tests::chain::{derive_mock_lc_pdas, ChainAccounts};
use solana_sdk::{signer::Signer, transaction::Transaction};

/// Three-chain roundtrip: A→B then B→C, with independent GMP lifecycles on
/// each hop.
///
/// Chain A: client `"a-to-b"` ↔ counterparty `"b-to-a"`
/// Chain B: primary `"b-to-a"` ↔ `"a-to-b"`, additional `"b-to-c"` ↔ `"c-to-b"`
/// Chain C: client `"c-to-b"` ↔ counterparty `"b-to-c"`
#[tokio::test]
async fn test_gmp_three_chain_roundtrip() {
    let user = User::new();
    let relayer = Relayer::new();
    let deployer = Deployer::new();
    let admin = Admin::new();
    let proof_data = vec![0u8; 32];

    // ── Chain A ──
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "a-to-b",
        counterparty_client_id: "b-to-a",
        deployer: &deployer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_a.prefund(&[&admin, &relayer, &user]);

    // ── Chain B (dual client) ──
    let mut chain_b = Chain::new(ChainConfig {
        client_id: "b-to-a",
        counterparty_client_id: "a-to-b",
        deployer: &deployer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_b.prefund(&[&admin, &relayer, &user]);

    // ── Chain C ──
    let mut chain_c = Chain::new(ChainConfig {
        client_id: "c-to-b",
        counterparty_client_id: "b-to-c",
        deployer: &deployer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_c.prefund(&[&admin, &relayer]);

    // Derive GMP PDAs for each hop
    // Leg 1: A→B — GMP account on Chain B derived from b-to-a client + user
    let gmp_pda_on_b = gmp::derive_gmp_account_pda("b-to-a", &user.pubkey());
    chain_b.prefund_lamports(gmp_pda_on_b, 10_000_000);
    let counter_on_b = gmp::derive_user_counter_pda(&gmp_pda_on_b);
    let counter_state_b = chain_b.counter_app_state_pda();

    // Leg 2: B→C — GMP account on Chain C derived from c-to-b client + user
    let gmp_pda_on_c = gmp::derive_gmp_account_pda("c-to-b", &user.pubkey());
    chain_c.prefund_lamports(gmp_pda_on_c, 10_000_000);
    let counter_on_c = gmp::derive_user_counter_pda(&gmp_pda_on_c);
    let counter_state_c = chain_c.counter_app_state_pda();

    // Build payloads
    let amount_a_to_b = 42u64;
    let payload_ab =
        gmp::encode_increment_payload(counter_state_b, counter_on_b, gmp_pda_on_b, amount_a_to_b);
    let packet_ab = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &payload_ab);

    let amount_b_to_c = 58u64;
    let payload_bc =
        gmp::encode_increment_payload(counter_state_c, counter_on_c, gmp_pda_on_c, amount_b_to_c);
    let packet_bc = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &payload_bc);

    // ── Start all chains ──
    chain_a.start().await;
    deployer.init_programs(&mut chain_a, &admin, &relayer).await;
    deployer.transfer_upgrade_authority(&mut chain_a).await;
    chain_b.start().await;
    deployer.init_programs(&mut chain_b, &admin, &relayer).await;
    deployer
        .add_counterparty(&mut chain_b, &admin, "b-to-c", "c-to-b")
        .await;
    deployer.transfer_upgrade_authority(&mut chain_b).await;
    chain_c.start().await;
    deployer.init_programs(&mut chain_c, &admin, &relayer).await;
    deployer.transfer_upgrade_authority(&mut chain_c).await;

    // ══════════════════════════════════════════════════════════════════════
    // Leg 1: A → B (sequence=1, amount=42)
    // ══════════════════════════════════════════════════════════════════════

    let commitment_a = user
        .send_call(
            &mut chain_a,
            GmpSendCallParams {
                sequence: 1,
                timeout_timestamp: GMP_TIMEOUT,
                receiver: &test_gmp_app::ID.to_string(),
                payload: payload_ab.encode_to_vec(),
            },
        )
        .await
        .expect("A→B send_call failed");

    // Recv on Chain B
    let (b_recv_payload, b_recv_proof) = relayer
        .upload_chunks(&mut chain_b, 1, &packet_ab, &proof_data)
        .await
        .expect("upload A→B recv chunks failed");

    let remaining_b =
        gmp::build_increment_remaining_accounts(gmp_pda_on_b, counter_state_b, counter_on_b);
    let recv_on_b = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence: 1,
                payload_chunk_pda: b_recv_payload,
                proof_chunk_pda: b_recv_proof,
                remaining_accounts: remaining_b,
            },
        )
        .await
        .expect("A→B recv_packet failed");

    let ack_b_data = extract_ack_data(&chain_b, recv_on_b.ack_pda).await;

    let (a_ack_payload, a_ack_proof) = relayer
        .upload_chunks(&mut chain_a, 1, &packet_ab, &proof_data)
        .await
        .expect("upload A→B ack chunks failed");

    relayer
        .gmp_ack_packet(
            &mut chain_a,
            GmpAckPacketParams {
                sequence: 1,
                acknowledgement: ack_b_data,
                payload_chunk_pda: a_ack_payload,
                proof_chunk_pda: a_ack_proof,
            },
        )
        .await
        .expect("A→B ack_packet failed");

    // ══════════════════════════════════════════════════════════════════════
    // Leg 2: B → C (sequence=1, amount=58)
    // ══════════════════════════════════════════════════════════════════════

    // Build send_call using the b-to-c client on Chain B.
    // Need custom ChainAccounts with b-to-c mock LC PDAs.
    let (btc_client_state, btc_consensus_state) = derive_mock_lc_pdas("b-to-c");
    let btc_accounts = ChainAccounts {
        mock_client_state: btc_client_state,
        mock_consensus_state: btc_consensus_state,
        ..chain_b.accounts
    };

    let payer_pubkey = chain_b.payer().pubkey();
    let (send_bc_ix, commitment_b) = gmp::build_gmp_send_call_ix(
        user.pubkey(),
        payer_pubkey,
        &btc_accounts,
        "b-to-c",
        GmpSendCallParams {
            sequence: 1,
            timeout_timestamp: GMP_TIMEOUT,
            receiver: &test_gmp_app::ID.to_string(),
            payload: payload_bc.encode_to_vec(),
        },
    );

    let tx = Transaction::new_signed_with_payer(
        &[send_bc_ix],
        Some(&payer_pubkey),
        &[chain_b.payer(), user.keypair()],
        chain_b.blockhash(),
    );
    chain_b
        .process_transaction(tx)
        .await
        .expect("B→C send_call failed");

    // Recv on Chain C
    let (c_recv_payload, c_recv_proof) = relayer
        .upload_chunks(&mut chain_c, 1, &packet_bc, &proof_data)
        .await
        .expect("upload B→C recv chunks failed");

    let remaining_c =
        gmp::build_increment_remaining_accounts(gmp_pda_on_c, counter_state_c, counter_on_c);
    let recv_on_c = relayer
        .gmp_recv_packet(
            &mut chain_c,
            GmpRecvPacketParams {
                sequence: 1,
                payload_chunk_pda: c_recv_payload,
                proof_chunk_pda: c_recv_proof,
                remaining_accounts: remaining_c,
            },
        )
        .await
        .expect("B→C recv_packet failed");

    let ack_c_data = extract_ack_data(&chain_c, recv_on_c.ack_pda).await;

    let (b_ack_payload, b_ack_proof) = relayer
        .upload_chunks_for_client(&mut chain_b, "b-to-c", 1, &packet_bc, &proof_data)
        .await
        .expect("upload B→C ack chunks failed");

    // Build ack with the b-to-c accounts
    let (ack_bc_ix, _ack_commitment_b) = gmp::build_gmp_ack_packet_ix(
        relayer.pubkey(),
        &btc_accounts,
        "b-to-c",
        "c-to-b",
        chain_b.clock_time(),
        GmpAckPacketParams {
            sequence: 1,
            acknowledgement: ack_c_data,
            payload_chunk_pda: b_ack_payload,
            proof_chunk_pda: b_ack_proof,
        },
    );
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

    // ══════════════════════════════════════════════════════════════════════
    // Assertions
    // ══════════════════════════════════════════════════════════════════════

    // Chain A: commitment zeroed, GMPCallResultAccount exists
    assert_commitment_zeroed(&chain_a, commitment_a).await;
    assert_gmp_result_exists(&chain_a, chain_a.client_id(), 1).await;

    // Chain B: UserCounter = 42 from leg 1
    let counter_b = read_user_counter(&chain_b, counter_on_b).await;
    assert_eq!(counter_b.count, amount_a_to_b);

    // Chain B: commitment for leg 2 zeroed
    assert_commitment_zeroed(&chain_b, commitment_b).await;

    // Chain C: UserCounter = 58 from leg 2
    let counter_c = read_user_counter(&chain_c, counter_on_c).await;
    assert_eq!(counter_c.count, amount_b_to_c);
}
