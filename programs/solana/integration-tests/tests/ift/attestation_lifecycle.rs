use super::*;
use integration_tests::{
    attestation as att_helpers, attestor::Attestors, programs::AttestationLc, router::PROOF_HEIGHT,
};
use solana_ibc_types::ics24;

/// IFT full lifecycle with attestation light client (2-of-2 quorum):
/// `ift_transfer` -> attestation ack -> `finalize_transfer`.
///
/// Mirrors `full_lifecycle` but replaces the mock LC with real attestation
/// proof verification, ensuring IFT works end-to-end when proofs are
/// actually validated.
#[tokio::test]
async fn test_ift_attestation_lifecycle() {
    // ── Attestors ──
    let attestors = Attestors::new(2);

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
    let attestation_lc = AttestationLc::new(&attestors);
    let ibc_programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &attestation_lc];
    let all_programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &Ift, &attestation_lc];
    let mut chain = Chain::single_with_lc(&deployer, all_programs, attestation::ID);
    chain.prefund(&[&admin, &relayer, &user, &ift_admin]);

    // ── Init ──
    chain.start().await;
    deployer
        .init_ibc_stack(&mut chain, &admin, &relayer, ibc_programs)
        .await;
    deployer
        .init_programs(&mut chain, ift_admin.pubkey(), &[&Ift])
        .await;
    deployer
        .transfer_upgrade_authority(&mut chain, all_programs)
        .await;

    // ── Setup: create token, register bridge, mint to user ──
    let (mint, user_ata) =
        setup_ift_chain(&mut chain, &ift_admin, &mint_keypair, user.pubkey()).await;
    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE);

    let mint_call_payload = ift::encode_evm_mint_call(ift::EVM_RECEIVER, TRANSFER_AMOUNT);
    let gmp_packet_bytes =
        ift::encode_ift_gmp_packet(ift::COUNTERPARTY_IFT_ADDRESS, mint_call_payload);

    // ── Update client (create consensus state at PROOF_HEIGHT) ──
    relayer
        .attestation_update_client(&mut chain, &attestors, PROOF_HEIGHT)
        .await
        .expect("update_client failed");

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

    let balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(balance, INITIAL_BALANCE - TRANSFER_AMOUNT);

    // ── Build attestation proof for ack ──
    let ack_data = ift::success_ack();
    let ack_commitment =
        ics24::packet_acknowledgement_commitment_bytes32(std::slice::from_ref(&ack_data))
            .expect("compute ack commitment");

    let ack_entry = att_helpers::ack_commitment_entry(
        chain.counterparty_client_id(),
        sequence,
        ack_commitment
            .as_slice()
            .try_into()
            .expect("ack should be 32 bytes"),
    );
    let ack_proof =
        att_helpers::build_packet_membership_proof(&attestors, PROOF_HEIGHT, &[ack_entry]);
    let ack_proof_bytes = att_helpers::serialize_proof(&ack_proof);

    // ── Relayer uploads chunks and delivers ack ──
    let (ack_payload_pda, ack_proof_pda) = relayer
        .upload_chunks(&mut chain, sequence, &gmp_packet_bytes, &ack_proof_bytes)
        .await
        .expect("upload ack chunks failed");

    let ack_commitment_pda = relayer
        .ift_gmp_ack_packet(
            &mut chain,
            IftGmpAckPacketParams {
                sequence,
                acknowledgement: ack_data,
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

    ift::assert_pending_transfer_closed(&chain, result.pending_transfer_pda).await;

    let final_balance = TokenKind::Spl.read_balance(&chain, user_ata).await;
    assert_eq!(final_balance, INITIAL_BALANCE - TRANSFER_AMOUNT);
}
