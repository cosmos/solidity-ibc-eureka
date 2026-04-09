use super::*;

#[tokio::test]
async fn test_send_call_timeout_too_long() {
    let user = User::new();
    let relayer = Relayer::new();
    let deployer = Deployer::new();
    let admin = Admin::new();
    let programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &TestGmpApp];

    let mut chain_a = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        programs,
    });
    chain_a.prefund(&[&admin, &relayer, &user]);

    // Build a minimal GMP payload (content doesn't matter — send will fail)
    let gmp_account_pda = gmp::derive_gmp_account_pda("chain-b-client", &user.pubkey());
    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_a.counter_app_state_pda();
    let payload =
        gmp::encode_increment_payload(counter_app_state, user_counter_pda, gmp_account_pda, 1);

    chain_a.start().await;
    deployer
        .init_programs(&mut chain_a, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs)
        .await;

    // ── Timeout at the exact boundary: rejected ──
    // GMP checks `timeout < current_time + MAX_TIMEOUT_DURATION` (strict <)
    let err = user
        .send_call(
            &mut chain_a,
            GmpSendCallParams {
                sequence: 1,
                timeout_timestamp: GMP_TIMEOUT_TOO_LONG,
                receiver: &test_gmp_app::ID.to_string(),
                payload: payload.encode_to_vec(),
            },
        )
        .await
        .expect_err("send_call with timeout at MAX boundary should fail");

    assert_eq!(
        extract_custom_error(&err),
        anchor_error_code(GMPError::TimeoutTooLong as u32),
    );

    // ── One second below the boundary: succeeds ──
    user.send_call(
        &mut chain_a,
        GmpSendCallParams {
            sequence: 1,
            timeout_timestamp: GMP_TIMEOUT_TOO_LONG.saturating_sub(1),
            receiver: &test_gmp_app::ID.to_string(),
            payload: payload.encode_to_vec(),
        },
    )
    .await
    .expect("send_call with timeout just below MAX should succeed");
}
