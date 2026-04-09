use super::*;
use integration_tests::extract_custom_error;

/// Pause blocks transfers and admin-mint; unpause restores them.
///
/// After the admin pauses the IFT app, both `ift_transfer` and `admin_mint`
/// should fail with `AppPaused`. Unpausing should restore normal operation.
#[tokio::test]
async fn test_ift_pause() {
    let user = User::new();
    let relayer = Relayer::new();
    let mint_keypair = Keypair::new();

    let mut chain = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        programs: &[Program::Ics27Gmp, Program::Ift],
    });
    chain.prefund(&user);
    chain.start().await;

    let (mint, user_ata) = setup_ift_chain(&mut chain, &mint_keypair, user.pubkey()).await;

    let authority_pubkey = chain.authority().pubkey();
    let payer_pubkey = chain.payer().pubkey();

    // ── Pause the app ──
    let pause_ix = ift::build_set_paused_ix(authority_pubkey, true);
    let tx = Transaction::new_signed_with_payer(
        &[pause_ix],
        Some(&payer_pubkey),
        &[chain.payer(), chain.authority()],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("pause");

    let state = ift::read_app_state(&chain).await;
    assert!(state.paused);

    // ── Transfer should fail while paused ──
    let err = user
        .ift_transfer(
            &mut chain,
            mint,
            TokenKind::Spl,
            IftTransferParams {
                sequence: 1,
                receiver: ift::EVM_RECEIVER.to_string(),
                amount: TRANSFER_AMOUNT,
                timeout_timestamp: IFT_TIMEOUT,
            },
        )
        .await
        .expect_err("transfer should fail when paused");

    let app_paused_code =
        integration_tests::anchor_error_code(::ift::errors::IFTError::AppPaused as u32);
    assert_eq!(extract_custom_error(&err), app_paused_code);

    // ── Admin mint should fail while paused ──
    let admin_mint_ix = ift::build_admin_mint_ix(
        authority_pubkey,
        payer_pubkey,
        mint,
        user.pubkey(),
        100,
        TokenKind::Spl,
    );
    let tx = Transaction::new_signed_with_payer(
        &[admin_mint_ix],
        Some(&payer_pubkey),
        &[chain.payer(), chain.authority()],
        chain.blockhash(),
    );
    let err = chain
        .process_transaction(tx)
        .await
        .expect_err("admin_mint should fail when paused");
    assert_eq!(extract_custom_error(&err), app_paused_code);

    // Balance unchanged
    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE);

    // ── Unpause ──
    let unpause_ix = ift::build_set_paused_ix(authority_pubkey, false);
    let tx = Transaction::new_signed_with_payer(
        &[unpause_ix],
        Some(&payer_pubkey),
        &[chain.payer(), chain.authority()],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("unpause");

    let state = ift::read_app_state(&chain).await;
    assert!(!state.paused);

    // ── Transfer should succeed after unpause ──
    user.ift_transfer(
        &mut chain,
        mint,
        TokenKind::Spl,
        IftTransferParams {
            sequence: 1,
            receiver: ift::EVM_RECEIVER.to_string(),
            amount: TRANSFER_AMOUNT,
            timeout_timestamp: IFT_TIMEOUT,
        },
    )
    .await
    .expect("transfer should succeed after unpause");

    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE - TRANSFER_AMOUNT);
}
