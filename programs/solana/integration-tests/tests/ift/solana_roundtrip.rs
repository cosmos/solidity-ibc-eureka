use super::*;

/// Solana↔Solana IFT roundtrip.
///
/// Two independent `ProgramTest` chains each run the IFT + GMP stack. The
/// source chain A burns the user's tokens and dispatches a `RawGmpSolanaPayload`
/// whose target is chain B's `ift_mint`. The relayer uploads chunks and
/// invokes `gmp_recv_packet` on B; the GMP program signs the CPI into IFT's
/// `ift_mint` which creates the receiver ATA and mints `TRANSFER_AMOUNT`
/// tokens. A success ack is delivered back to A, `ift_finalize_transfer`
/// closes the `PendingTransfer`, and A's token balance stays at
/// `INITIAL_BALANCE - TRANSFER_AMOUNT` (tokens remain burned on success).
#[tokio::test]
async fn test_ift_solana_roundtrip() {
    // ── Actors ──
    let deployer = Deployer::new();
    let admin = Admin::new();
    let ift_admin = IftAdmin::new();
    let relayer = Relayer::new();
    let user = User::new();
    let mint_keypair_a = Keypair::new();
    let mint_keypair_b = Keypair::new();

    // ── Test data ──
    let sequence = 1u64;

    // ── Chains ──
    let programs: &[&dyn ChainProgram] = &[&Ics27Gmp, &Ift];
    let (mut chain_a, mut chain_b) = Chain::pair(&deployer, programs);
    chain_a.prefund(&[&admin, &ift_admin, &relayer, &user]);
    chain_b.prefund(&[&admin, &ift_admin, &relayer, &user]);

    // Prefund the destination-side GMP account PDA so it can pay ATA-init rent
    // for the receiver during `ift_mint`. The PDA is derived with chain B's
    // local client_id and the source IFT program ID as the sender string —
    // matching both the program's `construct_solana_mint_call` and GMP's
    // `on_recv_packet` dispatch PDA.
    let gmp_account_pda_on_b = gmp::derive_gmp_account_pda(chain_b.client_id(), &::ift::ID);
    chain_b.prefund_lamports(gmp_account_pda_on_b, GMP_ACCOUNT_PREFUND_LAMPORTS);

    // ── Init ──
    init_ift_chain(
        &mut chain_a,
        &deployer,
        &admin,
        &ift_admin,
        &relayer,
        programs,
    )
    .await;
    init_ift_chain(
        &mut chain_b,
        &deployer,
        &admin,
        &ift_admin,
        &relayer,
        programs,
    )
    .await;

    // ── Setup tokens ──
    // Chain A: create mint, register Solana-target bridge, admin-mint to user.
    let mint_a = mint_keypair_a.pubkey();
    let mint_b = mint_keypair_b.pubkey();

    create_spl_token(&mut chain_a, &ift_admin, &mint_keypair_a).await;
    create_spl_token(&mut chain_b, &ift_admin, &mint_keypair_b).await;

    let chain_a_client_id = chain_a.client_id().to_string();
    let chain_a_counterparty_client_id = chain_a.counterparty_client_id().to_string();
    let chain_b_client_id = chain_b.client_id().to_string();
    let chain_b_counterparty_client_id = chain_b.counterparty_client_id().to_string();

    register_solana_bridge(
        &mut chain_a,
        &ift_admin,
        mint_a,
        chain_a_client_id,
        mint_b,
        chain_a_counterparty_client_id,
    )
    .await;
    register_solana_bridge(
        &mut chain_b,
        &ift_admin,
        mint_b,
        chain_b_client_id,
        mint_a,
        chain_b_counterparty_client_id,
    )
    .await;

    admin_mint_to_user(&mut chain_a, &ift_admin, mint_a, user.pubkey()).await;
    let user_ata_a = TokenKind::Spl.get_ata(&user.pubkey(), &mint_a);
    assert_eq!(
        TokenKind::Spl.read_balance(&chain_a, user_ata_a).await,
        INITIAL_BALANCE
    );

    // ── Build the Solana-targeted mint payload + GMP packet bytes ──
    // The packet bytes are needed by the relayer for both `recv_packet` on B
    // (as the uploaded chunk body) and `ack_packet` on A (same bytes — GMP
    // reconstructs the packet hash from the ack payload).
    let solana_payload = ift::encode_ift_solana_mint_payload(
        ::ift::ID,
        mint_b,
        chain_b.client_id(),
        ::ift::ID,
        user.pubkey(),
        TRANSFER_AMOUNT,
    );
    let gmp_packet_bytes = ift::encode_ift_solana_gmp_packet(::ift::ID, ::ift::ID, &solana_payload);

    // ── User sends IFT transfer on chain A ──
    let result = user
        .ift_transfer(
            &mut chain_a,
            mint_a,
            TokenKind::Spl,
            IftTransferParams {
                sequence,
                receiver: user.pubkey().to_string(),
                amount: TRANSFER_AMOUNT,
                timeout_timestamp: IFT_TIMEOUT,
            },
        )
        .await
        .expect("ift_transfer on chain A failed");

    assert_commitment_set(&chain_a, result.commitment_pda).await;
    assert_eq!(
        TokenKind::Spl.read_balance(&chain_a, user_ata_a).await,
        INITIAL_BALANCE - TRANSFER_AMOUNT,
        "tokens must be burned on source after ift_transfer",
    );

    // ── Relayer delivers recv_packet to chain B (mints tokens to user) ──
    let (b_recv_payload_chunk, b_recv_proof_chunk) = relayer
        .upload_chunks(&mut chain_b, sequence, &gmp_packet_bytes, DUMMY_PROOF)
        .await
        .expect("upload recv chunks on chain B failed");

    let remaining_accounts =
        ift::build_ift_solana_remaining_accounts(gmp_account_pda_on_b, ::ift::ID, &solana_payload);

    let recv = relayer
        .gmp_recv_packet(
            &mut chain_b,
            GmpRecvPacketParams {
                sequence,
                payload_chunk_pda: b_recv_payload_chunk,
                proof_chunk_pda: b_recv_proof_chunk,
                remaining_accounts,
            },
        )
        .await
        .expect("gmp_recv_packet on chain B failed");

    // ── Assert: receiver ATA was created and credited on chain B ──
    let user_ata_b = TokenKind::Spl.get_ata(&user.pubkey(), &mint_b);
    assert_eq!(
        TokenKind::Spl.read_balance(&chain_b, user_ata_b).await,
        TRANSFER_AMOUNT,
        "receiver should hold freshly-minted tokens on destination",
    );

    // ── Relayer delivers success ack back to chain A ──
    let ack_data = extract_ack_data(&chain_b, recv.ack_pda).await;
    let (a_ack_payload_chunk, a_ack_proof_chunk) = relayer
        .upload_chunks(&mut chain_a, sequence, &gmp_packet_bytes, DUMMY_PROOF)
        .await
        .expect("upload ack chunks on chain A failed");

    let ack_commitment_pda = relayer
        .gmp_ack_packet(
            &mut chain_a,
            GmpAckPacketParams {
                sequence,
                acknowledgement: ack_data,
                payload_chunk_pda: a_ack_payload_chunk,
                proof_chunk_pda: a_ack_proof_chunk,
            },
        )
        .await
        .expect("gmp_ack_packet on chain A failed");

    assert_commitment_zeroed(&chain_a, ack_commitment_pda).await;

    // ── Relayer finalizes the transfer on chain A (no refund for success) ──
    let client_id = chain_a.client_id().to_string();
    relayer
        .ift_finalize_transfer(
            &mut chain_a,
            mint_a,
            user.pubkey(),
            &client_id,
            sequence,
            TokenKind::Spl,
        )
        .await
        .expect("ift_finalize_transfer on chain A failed");

    ift::assert_pending_transfer_closed(&chain_a, result.pending_transfer_pda).await;
    assert_eq!(
        TokenKind::Spl.read_balance(&chain_a, user_ata_a).await,
        INITIAL_BALANCE - TRANSFER_AMOUNT,
        "source balance must remain burned after successful finalize",
    );
    // Destination balance untouched by the ack/finalize round-trip.
    assert_eq!(
        TokenKind::Spl.read_balance(&chain_b, user_ata_b).await,
        TRANSFER_AMOUNT,
    );
}

/// Create an SPL Token mint under IFT control.
async fn create_spl_token(chain: &mut Chain, ift_admin: &IftAdmin, mint_keypair: &Keypair) {
    let admin_pubkey = ift_admin.pubkey();
    let ix = ift::build_create_spl_token_ix(
        admin_pubkey,
        admin_pubkey,
        mint_keypair.pubkey(),
        MINT_DECIMALS,
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&admin_pubkey),
        &[ift_admin.keypair(), mint_keypair],
        chain.blockhash(),
    );
    chain
        .process_transaction(tx)
        .await
        .expect("create SPL token");
}

/// Register an IFT bridge whose counterparty is another Solana IFT program.
async fn register_solana_bridge(
    chain: &mut Chain,
    ift_admin: &IftAdmin,
    mint: Pubkey,
    client_id: String,
    counterparty_mint: Pubkey,
    counterparty_client_id: String,
) {
    let admin_pubkey = ift_admin.pubkey();
    let ix = ift::build_register_bridge_solana_ix(
        admin_pubkey,
        admin_pubkey,
        mint,
        &client_id,
        ::ift::ID,
        counterparty_mint,
        &counterparty_client_id,
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&admin_pubkey),
        &[ift_admin.keypair()],
        chain.blockhash(),
    );
    chain
        .process_transaction(tx)
        .await
        .expect("register Solana IFT bridge");
}

/// Mint `INITIAL_BALANCE` tokens to the user's ATA via the IFT admin.
async fn admin_mint_to_user(
    chain: &mut Chain,
    ift_admin: &IftAdmin,
    mint: Pubkey,
    user_pubkey: Pubkey,
) {
    let admin_pubkey = ift_admin.pubkey();
    let ix = ift::build_admin_mint_ix(
        admin_pubkey,
        admin_pubkey,
        mint,
        user_pubkey,
        INITIAL_BALANCE,
        TokenKind::Spl,
    );
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&admin_pubkey),
        &[ift_admin.keypair()],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("admin mint");
}
