use super::*;
use solana_sdk::pubkey::Pubkey;

struct LifecycleTarget {
    gmp_pda: Pubkey,
    counter_pda: Pubkey,
    counter_app_state: Pubkey,
}

/// Run one full send → recv → ack lifecycle, cleaning up ack chunks afterward.
#[allow(clippy::too_many_arguments)]
async fn lifecycle(
    user: &User,
    relayer: &Relayer,
    chain_a: &mut Chain,
    chain_b: &mut Chain,
    sequence: u64,
    amount: u64,
    target: &LifecycleTarget,
    proof_data: &[u8],
) {
    let payload = gmp::encode_increment_payload(
        target.counter_app_state,
        target.counter_pda,
        target.gmp_pda,
        amount,
    );
    let packet = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &payload);

    user.send_call(
        chain_a,
        GmpSendCallParams {
            sequence,
            timeout_timestamp: GMP_TIMEOUT,
            receiver: &test_gmp_app::ID.to_string(),
            payload: payload.encode_to_vec(),
        },
    )
    .await
    .expect("send_call failed");

    let (b_payload_pda, b_proof_pda) = relayer
        .upload_chunks(chain_b, sequence, &packet, proof_data)
        .await
        .expect("upload recv chunks failed");

    let remaining = gmp::build_increment_remaining_accounts(
        target.gmp_pda,
        target.counter_app_state,
        target.counter_pda,
    );

    let recv = relayer
        .gmp_recv_packet(
            chain_b,
            GmpRecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload_pda,
                proof_chunk_pda: b_proof_pda,
                remaining_accounts: remaining,
            },
        )
        .await
        .expect("recv_packet failed");

    let ack_data = extract_ack_data(chain_b, recv.ack_pda).await;

    let (a_payload_pda, a_proof_pda) = relayer
        .upload_chunks(chain_a, sequence, &packet, proof_data)
        .await
        .expect("upload ack chunks failed");

    relayer
        .gmp_ack_packet(
            chain_a,
            GmpAckPacketParams {
                sequence,
                acknowledgement: ack_data,
                payload_chunk_pda: a_payload_pda,
                proof_chunk_pda: a_proof_pda,
            },
        )
        .await
        .expect("ack_packet failed");

    relayer
        .cleanup_chunks(chain_a, sequence, a_payload_pda, a_proof_pda)
        .await
        .expect("cleanup ack chunks failed");
}

/// Two independent users send GMP calls through the same chain pair.
/// Each user gets their own `UserCounter` PDA and `GMPCallResultAccount`.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_gmp_multi_user_isolation() {
    let user_a = User::new();
    let user_b = User::new();
    let relayer = Relayer::new();
    let deployer = Deployer::new();
    let admin = Admin::new();
    let proof_data = vec![0u8; 32];

    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_a.prefund(&[&admin, &relayer, &user_a, &user_b]);

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        deployer: &deployer,
        programs: &[Program::Ics27Gmp, Program::TestGmpApp],
    });
    chain_b.prefund(&[&admin, &relayer]);

    let gmp_pda_a = gmp::derive_gmp_account_pda(chain_b.client_id(), &user_a.pubkey());
    let gmp_pda_b = gmp::derive_gmp_account_pda(chain_b.client_id(), &user_b.pubkey());
    chain_b.prefund_lamports(gmp_pda_a, 10_000_000);
    chain_b.prefund_lamports(gmp_pda_b, 10_000_000);

    let counter_pda_a = gmp::derive_user_counter_pda(&gmp_pda_a);
    let counter_pda_b = gmp::derive_user_counter_pda(&gmp_pda_b);
    let counter_app_state = chain_b.counter_app_state_pda();

    let target_a = LifecycleTarget {
        gmp_pda: gmp_pda_a,
        counter_pda: counter_pda_a,
        counter_app_state,
    };
    let target_b = LifecycleTarget {
        gmp_pda: gmp_pda_b,
        counter_pda: counter_pda_b,
        counter_app_state,
    };

    chain_a.start().await;
    deployer.init_programs(&mut chain_a, &admin, &relayer).await;
    deployer.transfer_upgrade_authority(&mut chain_a).await;
    chain_b.start().await;
    deployer.init_programs(&mut chain_b, &admin, &relayer).await;
    deployer.transfer_upgrade_authority(&mut chain_b).await;

    // ── user_a: seq=1, amount=5 ──
    lifecycle(
        &user_a,
        &relayer,
        &mut chain_a,
        &mut chain_b,
        1,
        5,
        &target_a,
        &proof_data,
    )
    .await;

    // ── user_a: seq=2, amount=7 ──
    lifecycle(
        &user_a,
        &relayer,
        &mut chain_a,
        &mut chain_b,
        2,
        7,
        &target_a,
        &proof_data,
    )
    .await;

    // ── user_b: seq=3, amount=3 ──
    lifecycle(
        &user_b,
        &relayer,
        &mut chain_a,
        &mut chain_b,
        3,
        3,
        &target_b,
        &proof_data,
    )
    .await;

    // ── Assertions ──

    let counter_a = read_user_counter(&chain_b, counter_pda_a).await;
    assert_eq!(counter_a.count, 12, "user_a counter should be 5 + 7 = 12");

    let counter_b = read_user_counter(&chain_b, counter_pda_b).await;
    assert_eq!(counter_b.count, 3, "user_b counter should be 3");

    let state = read_counter_app_state(&chain_b, counter_app_state).await;
    assert_eq!(state.total_counters, 2, "should have 2 separate counters");

    for seq in [1u64, 2, 3] {
        assert_gmp_result_exists(&chain_a, chain_a.client_id(), seq).await;
    }
}
