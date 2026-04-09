use super::*;

/// Two-step admin transfer: propose -> accept. Then propose -> cancel.
///
/// Verifies that admin ownership can be transferred via the two-step process
/// and that a pending proposal can be cancelled by the current admin.
#[tokio::test]
async fn test_ift_admin_transfer() {
    let relayer = Relayer::new();
    let new_admin = Keypair::new();
    let another_admin = Keypair::new();

    let mut chain = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        relayer: &relayer,
        programs: &[Program::Ics27Gmp, Program::Ift],
    });
    chain.start().await;

    let authority_pubkey = chain.authority().pubkey();
    let payer_pubkey = chain.payer().pubkey();

    // ── Verify initial state ──
    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.admin, authority_pubkey);
    assert!(state.pending_admin.is_none());

    // ── Propose new admin ──
    let propose_ix = ift::build_propose_admin_ix(authority_pubkey, new_admin.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[propose_ix],
        Some(&payer_pubkey),
        &[chain.payer(), chain.authority()],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("propose admin");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.pending_admin, Some(new_admin.pubkey()));
    assert_eq!(state.admin, authority_pubkey);

    // ── New admin accepts ──
    let accept_ix = ift::build_accept_admin_ix(new_admin.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[accept_ix],
        Some(&payer_pubkey),
        &[chain.payer(), &new_admin],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("accept admin");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.admin, new_admin.pubkey());
    assert!(state.pending_admin.is_none());

    // ── New admin proposes another admin, then cancels ──
    let propose_ix = ift::build_propose_admin_ix(new_admin.pubkey(), another_admin.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[propose_ix],
        Some(&payer_pubkey),
        &[chain.payer(), &new_admin],
        chain.blockhash(),
    );
    chain
        .process_transaction(tx)
        .await
        .expect("propose another admin");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.pending_admin, Some(another_admin.pubkey()));

    let cancel_ix = ift::build_cancel_admin_proposal_ix(new_admin.pubkey());
    let tx = Transaction::new_signed_with_payer(
        &[cancel_ix],
        Some(&payer_pubkey),
        &[chain.payer(), &new_admin],
        chain.blockhash(),
    );
    chain
        .process_transaction(tx)
        .await
        .expect("cancel proposal");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.admin, new_admin.pubkey());
    assert!(state.pending_admin.is_none());
}
