use super::*;
use integration_tests::{admin::Admin, ift_admin::IftAdmin};

/// Two-step admin transfer: propose -> accept. Then propose -> cancel.
///
/// Verifies that admin ownership can be transferred via the two-step process
/// and that a pending proposal can be cancelled by the current admin.
#[tokio::test]
async fn test_ift_admin_transfer() {
    let relayer = Relayer::new();
    let new_admin_keypair = Keypair::new();
    let another_admin_keypair = Keypair::new();

    let deployer = Deployer::new();
    let admin = Admin::new();
    let mut chain = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        programs: &[Program::Ics27Gmp, Program::Ift],
    });
    chain.prefund(&[&admin, &relayer]);
    chain.start().await;
    deployer.init_programs(&mut chain, &admin, &relayer).await;
    deployer.transfer_upgrade_authority(&mut chain).await;

    let ift_admin = IftAdmin::from_keypair(admin.keypair().insecure_clone());

    // ── Verify initial state ──
    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.admin, ift_admin.pubkey());
    assert!(state.pending_admin.is_none());

    // ── Propose new admin ──
    ift_admin
        .propose_admin(&mut chain, new_admin_keypair.pubkey())
        .await
        .expect("propose admin");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.pending_admin, Some(new_admin_keypair.pubkey()));
    assert_eq!(state.admin, ift_admin.pubkey());

    // ── New admin accepts ──
    let new_admin = IftAdmin::from_keypair(new_admin_keypair);
    new_admin
        .accept_admin(&mut chain)
        .await
        .expect("accept admin");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.admin, new_admin.pubkey());
    assert!(state.pending_admin.is_none());

    // ── New admin proposes another admin, then cancels ──
    new_admin
        .propose_admin(&mut chain, another_admin_keypair.pubkey())
        .await
        .expect("propose another admin");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.pending_admin, Some(another_admin_keypair.pubkey()));

    new_admin
        .cancel_admin_proposal(&mut chain)
        .await
        .expect("cancel proposal");

    let state = ift::read_app_state(&chain).await;
    assert_eq!(state.admin, new_admin.pubkey());
    assert!(state.pending_admin.is_none());
}
