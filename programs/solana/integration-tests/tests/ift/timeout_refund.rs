use super::*;

/// IFT timeout refund: `ift_transfer` -> timeout -> `finalize_transfer`.
///
/// The user transfers tokens via IFT. The packet times out and the relayer
/// delivers a timeout. `finalize_transfer` refunds the burned tokens back
/// to the user.
#[tokio::test]
async fn test_ift_timeout_refund() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let ift_admin = IftAdmin::new();
    let relayer = Relayer::new();
    let user = User::new();
    let mint_keypair = Keypair::new();

    // ── Test data ──
    let sequence = 1u64;

    // ── Chain ──
    let programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &Ift];
    let mut chain = Chain::single(&deployer, programs);
    chain.prefund(&[&admin, &relayer, &user, &ift_admin]);

    // ── Init ──
    chain.start().await;
    deployer
        .init_ibc_stack(&mut chain, &admin, &relayer, &[&Ics27Gmp])
        .await;
    deployer
        .init_programs(&mut chain, ift_admin.pubkey(), &[&Ift])
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain, programs)
        .await;

    // ── Setup ──
    let (mint, user_ata) =
        setup_ift_chain(&mut chain, &ift_admin, &mint_keypair, user.pubkey()).await;
    let mint_call_payload = ift::encode_evm_mint_call(ift::EVM_RECEIVER, TRANSFER_AMOUNT);
    let gmp_packet_bytes =
        ift::encode_ift_gmp_packet(ift::COUNTERPARTY_IFT_ADDRESS, mint_call_payload);

    // ── User sends IFT transfer ──
    let result = user
        .ift_transfer(
            &mut chain,
            mint,
            TokenKind::Spl,
            IftTransferParams {
                sequence,
                receiver: ift::EVM_RECEIVER.to_string(),
                amount: TRANSFER_AMOUNT,
                timeout_timestamp: IFT_TIMEOUT,
            },
        )
        .await
        .expect("ift_transfer failed");

    assert_commitment_set(&chain, result.commitment_pda).await;

    // Tokens burned
    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE - TRANSFER_AMOUNT);

    // ── Relayer uploads chunks and delivers timeout ──
    let (timeout_payload_pda, timeout_proof_pda) = relayer
        .upload_chunks(&mut chain, sequence, &gmp_packet_bytes, DUMMY_PROOF)
        .await
        .expect("upload timeout chunks failed");

    let timeout_commitment_pda = relayer
        .ift_gmp_timeout_packet(
            &mut chain,
            IftGmpTimeoutPacketParams {
                sequence,
                payload_chunk_pda: timeout_payload_pda,
                proof_chunk_pda: timeout_proof_pda,
            },
        )
        .await
        .expect("ift_gmp_timeout_packet failed");

    assert_commitment_zeroed(&chain, timeout_commitment_pda).await;

    // ── Relayer finalizes transfer (refund for timeout) ──
    let client_id = chain.client_id().to_string();
    relayer
        .ift_finalize_transfer(
            &mut chain,
            mint,
            user.pubkey(),
            &client_id,
            sequence,
            TokenKind::Spl,
        )
        .await
        .expect("finalize_transfer failed");

    // PendingTransfer closed
    ift::assert_pending_transfer_closed(&chain, result.pending_transfer_pda).await;

    // Tokens refunded back to user
    let final_balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(final_balance, INITIAL_BALANCE);
}
