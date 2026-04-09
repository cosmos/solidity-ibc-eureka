use super::*;

/// IFT full lifecycle: `ift_transfer` -> success ack -> `finalize_transfer`.
///
/// The user transfers tokens via IFT which burns them locally and sends a
/// GMP packet. The relayer delivers a success ack, and `finalize_transfer`
/// confirms the burn (no refund). The user's final balance should be
/// `INITIAL_BALANCE - TRANSFER_AMOUNT`.
#[tokio::test]
async fn test_ift_full_lifecycle() {
    let user = User::new();
    let relayer = Relayer::new();
    let mint_keypair = Keypair::new();
    let proof_data = vec![0u8; 32];
    let sequence = 1u64;

    let admin = Admin::new();
    let mut chain = Chain::new(ChainConfig {
        client_id: "chain-a-client",
        counterparty_client_id: "chain-b-client",
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::Ics27Gmp, Program::Ift],
    });
    chain.prefund(&user);
    chain.start().await;

    let (mint, user_ata) = setup_ift_chain(&mut chain, &mint_keypair, user.pubkey()).await;

    // Verify initial balance
    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE);

    // Build expected GMP packet bytes for ack delivery
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

    // Tokens should be burned
    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE - TRANSFER_AMOUNT);

    // ── Relayer uploads chunks and delivers success ack ──
    let (ack_payload_pda, ack_proof_pda) = relayer
        .upload_chunks(&mut chain, sequence, &gmp_packet_bytes, &proof_data)
        .await
        .expect("upload ack chunks failed");

    let ack_commitment_pda = relayer
        .ift_gmp_ack_packet(
            &mut chain,
            IftGmpAckPacketParams {
                sequence,
                acknowledgement: ift::success_ack(),
                payload_chunk_pda: ack_payload_pda,
                proof_chunk_pda: ack_proof_pda,
            },
        )
        .await
        .expect("ift_gmp_ack_packet failed");

    assert_commitment_zeroed(&chain, ack_commitment_pda).await;

    // ── Relayer finalizes transfer (no refund for success) ──
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

    // Balance unchanged after finalize (tokens stay burned)
    let final_balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(final_balance, INITIAL_BALANCE - TRANSFER_AMOUNT);
}
