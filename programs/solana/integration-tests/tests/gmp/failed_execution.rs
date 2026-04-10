use super::*;
use solana_ibc_proto::{RawGmpSolanaPayload, RawSolanaAccountMeta};
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey, system_program};

/// When the target CPI fails (wrong PDA), the entire recv transaction reverts
/// and no receipt or ack is created.
#[tokio::test]
async fn test_gmp_failed_execution_aborts() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let relayer = Relayer::new();
    let user = User::new();

    // ── Test data ──
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;
    let increment_amount = 10u64;

    // ── Chains ──
    let programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &TestGmpApp];
    let (mut chain_a, mut chain_b) = Chain::pair(&deployer, programs);
    chain_a.prefund(&[&admin, &relayer, &user]);
    chain_b.prefund(&[&admin, &relayer]);

    let gmp_account_pda = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    chain_b.prefund_lamports(gmp_account_pda, 10_000_000);

    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);

    let fake_counter_app_state = Pubkey::new_unique();
    chain_b.prefund_lamports(fake_counter_app_state, 1_000_000);

    let mut ix_data = integration_tests::accounts::anchor_discriminator("increment").to_vec();
    ix_data.extend_from_slice(&increment_amount.to_le_bytes());

    let bad_payload = RawGmpSolanaPayload {
        data: ix_data,
        accounts: vec![
            RawSolanaAccountMeta {
                pubkey: fake_counter_app_state.to_bytes().to_vec(),
                is_signer: false,
                is_writable: true,
            },
            RawSolanaAccountMeta {
                pubkey: user_counter_pda.to_bytes().to_vec(),
                is_signer: false,
                is_writable: true,
            },
            RawSolanaAccountMeta {
                pubkey: gmp_account_pda.to_bytes().to_vec(),
                is_signer: true,
                is_writable: false,
            },
            RawSolanaAccountMeta {
                pubkey: gmp_account_pda.to_bytes().to_vec(),
                is_signer: true,
                is_writable: true,
            },
            RawSolanaAccountMeta {
                pubkey: system_program::ID.to_bytes().to_vec(),
                is_signer: false,
                is_writable: false,
            },
        ],
        prefund_lamports: 5_000_000,
    };

    let gmp_packet_bytes = gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &bad_payload);

    // ── Init ──
    chain_a.start().await;
    deployer
        .init_ibc_stack(&mut chain_a, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_a, programs)
        .await;
    chain_b.start().await;
    deployer
        .init_ibc_stack(&mut chain_b, &admin, &relayer, programs)
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain_b, programs)
        .await;

    // Send on Chain A
    user.send_call(
        &mut chain_a,
        GmpSendCallParams {
            sequence,
            timeout_timestamp: GMP_TIMEOUT,
            receiver: &test_gmp_app::ID.to_string(),
            payload: bad_payload.encode_to_vec(),
        },
    )
    .await
    .expect("send_call failed");

    // Upload chunks and attempt recv on Chain B
    let (b_payload, b_proof) = relayer
        .upload_chunks(&mut chain_b, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload recv chunks failed");

    let remaining = vec![
        AccountMeta::new(gmp_account_pda, false),
        AccountMeta::new_readonly(test_gmp_app::ID, false),
        AccountMeta::new(fake_counter_app_state, false),
        AccountMeta::new(user_counter_pda, false),
        AccountMeta::new(gmp_account_pda, false),
        AccountMeta::new(gmp_account_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let err = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence,
                payload_chunk_pda: b_payload,
                proof_chunk_pda: b_proof,
                remaining_accounts: remaining,
            },
        )
        .await
        .expect_err("recv_packet with bad PDA should fail");

    // The error is a CPI failure — check that it's a non-zero custom error
    let code = extract_custom_error(&err);
    assert_ne!(code, 0, "expected non-zero custom error from failed CPI");

    // No receipt or ack should exist on Chain B
    let receipt_pda = integration_tests::router::derive_receipt_pda(chain_b.client_id(), sequence);
    assert!(
        chain_b.get_account(receipt_pda).await.is_none(),
        "receipt should not exist after failed execution"
    );
}
