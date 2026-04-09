use super::*;

/// Two consecutive IFT transfers, both finalized with success acks.
///
/// Verifies that sequential sequences (1 and 2) each create independent
/// pending transfers and commitments, and that both can be finalized
/// independently.
#[tokio::test]
async fn test_ift_batch_transfers() {
    let user = User::new();
    let relayer = Relayer::new();
    let mint_keypair = Keypair::new();
    let proof_data = vec![0u8; 32];

    let deployer = Deployer::new();
    let admin = Admin::new();
    let mut chain = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        deployer: &deployer,
        programs: &[Program::Ics27Gmp, Program::Ift],
    });
    chain.prefund(&[&admin, &relayer, &user]);
    chain.start().await;
    deployer.init_programs(&mut chain, &admin, &relayer).await;
    deployer.transfer_upgrade_authority(&mut chain).await;

    let (mint, user_ata) = setup_ift_chain(&mut chain, &admin, &mint_keypair, user.pubkey()).await;

    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE);

    // ── Transfer #1 (sequence=1) ──
    let result_1 = user
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
        .expect("ift_transfer #1 failed");

    assert_commitment_set(&chain, result_1.commitment_pda).await;

    // ── Transfer #2 (sequence=2) ──
    let result_2 = user
        .ift_transfer(
            &mut chain,
            mint,
            TokenKind::Spl,
            IftTransferParams {
                sequence: 2,
                receiver: ift::EVM_RECEIVER.to_string(),
                amount: TRANSFER_AMOUNT,
                timeout_timestamp: IFT_TIMEOUT,
            },
        )
        .await
        .expect("ift_transfer #2 failed");

    assert_commitment_set(&chain, result_2.commitment_pda).await;

    // Both transfers burned
    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE - 2 * TRANSFER_AMOUNT);

    // ── Ack + finalize for transfer #1 ──
    let mint_call_1 = ift::encode_evm_mint_call(ift::EVM_RECEIVER, TRANSFER_AMOUNT);
    let gmp_packet_1 = ift::encode_ift_gmp_packet(ift::COUNTERPARTY_IFT_ADDRESS, mint_call_1);

    let (payload_pda_1, proof_pda_1) = relayer
        .upload_chunks(&mut chain, 1, &gmp_packet_1, &proof_data)
        .await
        .expect("upload chunks #1");

    relayer
        .ift_gmp_ack_packet(
            &mut chain,
            IftGmpAckPacketParams {
                sequence: 1,
                acknowledgement: ift::success_ack(),
                payload_chunk_pda: payload_pda_1,
                proof_chunk_pda: proof_pda_1,
            },
        )
        .await
        .expect("ack #1");

    let client_id = chain.client_id().to_string();
    relayer
        .ift_finalize_transfer(
            &mut chain,
            mint,
            user.pubkey(),
            &client_id,
            1,
            TokenKind::Spl,
        )
        .await
        .expect("finalize #1");

    ift::assert_pending_transfer_closed(&chain, result_1.pending_transfer_pda).await;

    // ── Ack + finalize for transfer #2 ──
    let mint_call_2 = ift::encode_evm_mint_call(ift::EVM_RECEIVER, TRANSFER_AMOUNT);
    let gmp_packet_2 = ift::encode_ift_gmp_packet(ift::COUNTERPARTY_IFT_ADDRESS, mint_call_2);

    let (payload_pda_2, proof_pda_2) = relayer
        .upload_chunks(&mut chain, 2, &gmp_packet_2, &proof_data)
        .await
        .expect("upload chunks #2");

    relayer
        .ift_gmp_ack_packet(
            &mut chain,
            IftGmpAckPacketParams {
                sequence: 2,
                acknowledgement: ift::success_ack(),
                payload_chunk_pda: payload_pda_2,
                proof_chunk_pda: proof_pda_2,
            },
        )
        .await
        .expect("ack #2");

    relayer
        .ift_finalize_transfer(
            &mut chain,
            mint,
            user.pubkey(),
            &client_id,
            2,
            TokenKind::Spl,
        )
        .await
        .expect("finalize #2");

    ift::assert_pending_transfer_closed(&chain, result_2.pending_transfer_pda).await;

    // Final balance: both transfers burned, no refunds
    let final_balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(final_balance, INITIAL_BALANCE - 2 * TRANSFER_AMOUNT);
}
