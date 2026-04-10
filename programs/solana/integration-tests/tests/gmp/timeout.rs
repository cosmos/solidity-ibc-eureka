use super::*;

/// GMP timeout lifecycle: `send_call` on A -> `timeout_packet` on A.
///
/// Chain A sends a GMP call but the packet is never delivered to Chain B.
/// The relayer delivers a timeout back to Chain A, which creates a
/// `GMPCallResultAccount` with timeout status.
#[tokio::test]
async fn test_gmp_timeout() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let increment_amount = 42u64;

    // ── Chain ──
    let programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &TestGmpApp];
    let mut chain_a = Chain::single(&deployer, programs);
    chain_a.prefund(&[&admin, &relayer, &user]);

    let gmp_account_pda = gmp::derive_gmp_account_pda("chain-b-client", &user.pubkey());
    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_a.counter_app_state_pda();

    let solana_payload = gmp::encode_increment_payload(
        counter_app_state,
        user_counter_pda,
        gmp_account_pda,
        increment_amount,
    );
    let gmp_packet_bytes =
        gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &solana_payload);

    // ── Init ──
    chain_a.start().await;
    deployer
        .init_ibc_stack(&mut chain_a, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs)
        .await;

    // ── User sends GMP call on Chain A ──
    let commitment_pda = user
        .send_call(
            &mut chain_a,
            GmpSendCallParams {
                sequence,
                timeout_timestamp: GMP_TIMEOUT,
                receiver: &test_gmp_app::ID.to_string(),
                payload: solana_payload.encode_to_vec(),
            },
        )
        .await
        .expect("send_call failed");

    assert_commitment_set(&chain_a, commitment_pda).await;

    // ── Relayer uploads chunks and delivers timeout on Chain A ──
    let (timeout_payload, timeout_proof) = relayer
        .upload_chunks(&mut chain_a, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload timeout chunks on Chain A failed");
    let timeout_commitment_pda = relayer
        .gmp_timeout_packet(
            &mut chain_a,
            gmp::GmpTimeoutPacketParams {
                sequence,
                payload_chunk_pda: timeout_payload,
                proof_chunk_pda: timeout_proof,
            },
        )
        .await
        .expect("gmp_timeout_packet failed");

    assert_commitment_zeroed(&chain_a, timeout_commitment_pda).await;

    // Verify GMPCallResultAccount was created with timeout status
    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(chain_a.client_id(), sequence, &ics27_gmp::ID);
    let result_account = chain_a
        .get_account(result_pda)
        .await
        .expect("GMPCallResultAccount should exist");
    let result_state =
        ics27_gmp::state::GMPCallResultAccount::try_deserialize(&mut &result_account.data[..])
            .expect("failed to deserialize GMPCallResultAccount");
    assert_eq!(
        result_state.status,
        solana_ibc_types::CallResultStatus::Timeout
    );
}
