use super::*;
use anchor_lang::InstructionData;
use solana_sdk::{instruction::AccountMeta, transaction::Transaction};

/// Calling `ics27_gmp::on_recv_packet` via CPI from an unauthorized program
/// (`test_cpi_proxy`, not `ics26_router`) is rejected with `UnauthorizedRouter`.
#[tokio::test]
async fn test_gmp_unauthorized_cpi_rejected() {
    let user = User::new();
    let relayer = Relayer::new();
    let sequence = 1u64;
    let increment_amount = 10u64;

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "chain-b-client",
        counterparty_client_id: "chain-a-client",
        relayer: &relayer,
        programs: &[
            Program::Ics27Gmp,
            Program::TestGmpApp,
            Program::TestCpiProxy,
        ],
    });

    let gmp_account_pda = gmp::derive_gmp_account_pda(chain_b.client_id(), &user.pubkey());
    chain_b.prefund_lamports(gmp_account_pda, 10_000_000);

    let user_counter_pda = gmp::derive_user_counter_pda(&gmp_account_pda);
    let counter_app_state = chain_b.counter_app_state_pda();

    let solana_payload = gmp::encode_increment_payload(
        counter_app_state,
        user_counter_pda,
        gmp_account_pda,
        increment_amount,
    );
    let gmp_packet_bytes =
        gmp::encode_gmp_packet(&user.pubkey(), &test_gmp_app::ID, &solana_payload);

    let gmp_remaining = gmp::build_increment_remaining_accounts(
        gmp_account_pda,
        counter_app_state,
        user_counter_pda,
    );

    chain_b.start().await;

    // Build the raw GMP on_recv_packet instruction
    let raw_ix = gmp::build_raw_gmp_on_recv_packet_ix(
        relayer.pubkey(),
        chain_b.client_id(),
        "chain-a-client",
        sequence,
        &gmp_packet_bytes,
        gmp_remaining.clone(),
    );

    // Build CpiAccountMeta descriptors for each account in the raw instruction
    let cpi_account_metas: Vec<test_cpi_proxy::CpiAccountMeta> = raw_ix
        .accounts
        .iter()
        .map(|m| test_cpi_proxy::CpiAccountMeta {
            is_signer: m.is_signer,
            is_writable: m.is_writable,
        })
        .collect();

    let proxy_data = test_cpi_proxy::instruction::ProxyCpi {
        instruction_data: raw_ix.data,
        account_metas: cpi_account_metas,
    }
    .data();

    // proxy_cpi accounts: target_program, payer, then remaining_accounts
    let mut proxy_accounts = vec![
        AccountMeta::new_readonly(ics27_gmp::ID, false), // target_program
        AccountMeta::new(relayer.pubkey(), true),        // payer
    ];
    // All accounts from the raw instruction become remaining_accounts
    for m in &raw_ix.accounts {
        if m.is_writable {
            proxy_accounts.push(AccountMeta::new(m.pubkey, false));
        } else {
            proxy_accounts.push(AccountMeta::new_readonly(m.pubkey, false));
        }
    }

    let proxy_ix = solana_sdk::instruction::Instruction {
        program_id: test_cpi_proxy::ID,
        accounts: proxy_accounts,
        data: proxy_data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[proxy_ix],
        Some(&relayer.pubkey()),
        &[relayer.keypair()],
        chain_b.blockhash(),
    );

    let err = chain_b
        .process_transaction(tx)
        .await
        .expect_err("CPI from unauthorized program should fail");

    assert_eq!(
        extract_custom_error(&err),
        anchor_error_code(GMPError::UnauthorizedRouter as u32),
        "expected UnauthorizedRouter error"
    );

    // No receipt PDA should have been created
    let receipt_pda = integration_tests::router::derive_receipt_pda(chain_b.client_id(), sequence);
    assert!(
        chain_b.get_account(receipt_pda).await.is_none(),
        "receipt should not exist after rejected unauthorized CPI"
    );
}
