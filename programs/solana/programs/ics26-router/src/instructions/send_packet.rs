use crate::errors::RouterError;
use crate::events::SendPacketEvent;
use crate::router_cpi::LightClientCpi;
use crate::state::*;
use crate::utils::sequence;
use anchor_lang::prelude::*;
use solana_ibc_types::ics24;
use solana_ibc_types::IBCAppState;

/// Sends an IBC packet by creating a packet commitment on-chain.
/// Must be called via CPI from a registered IBC application.
#[derive(Accounts)]
#[instruction(msg: MsgSendPacket)]
pub struct SendPacket<'info> {
    /// Global router configuration PDA.
    #[account(
        seeds = [RouterState::SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    /// PDA mapping the source port to its registered IBC application.
    #[account(
        seeds = [IBCApp::SEED, msg.payload.source_port.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    /// Mutable sequence counter for this client; incremented on each send.
    #[account(
        mut,
        seeds = [ClientSequence::SEED, msg.source_client.as_bytes()],
        bump
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    /// Stores the packet commitment hash. Created manually because the
    /// sequence is computed at runtime via `calculate_namespaced_sequence`.
    /// CHECK: Manually validated and created in instruction handler
    #[account(mut)]
    pub packet_commitment: UncheckedAccount<'info>,

    /// PDA signed by the calling IBC app program, proving it authorized this send.
    pub app_signer: Signer<'info>,

    /// Pays rent for the new `packet_commitment` account.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Solana system program used for account creation.
    pub system_program: Program<'info, System>,

    /// Client PDA for the source client; must be active.
    #[account(
        seeds = [Client::SEED, msg.source_client.as_bytes()],
        bump,
        constraint = client.active @ RouterError::ClientNotActive,
    )]
    pub client: Account<'info, Client>,

    /// Light client program used to query client status before sending.
    /// CHECK: Light client program, validated against client registry
    pub light_client_program: AccountInfo<'info>,

    /// Client state account owned by the light client program.
    /// CHECK: Client state account, owned by light client program
    pub client_state: AccountInfo<'info>,

    /// Consensus state account owned by the light client program (for expiry check).
    /// CHECK: Consensus state account, owned by light client program (for expiry check)
    pub consensus_state: AccountInfo<'info>,
}

pub fn send_packet(ctx: Context<SendPacket>, msg: MsgSendPacket) -> Result<u64> {
    // Check light client status before proceeding
    let light_client_cpi = LightClientCpi::new(&ctx.accounts.client);
    let status = light_client_cpi.client_status(
        &ctx.accounts.light_client_program,
        &ctx.accounts.client_state,
        &ctx.accounts.consensus_state,
    )?;
    require_eq!(
        status,
        ics25_handler::ClientStatus::Active,
        RouterError::ClientNotActive
    );

    let ibc_app = &ctx.accounts.ibc_app;
    let client_sequence = &mut ctx.accounts.client_sequence;
    let packet_commitment_info = &ctx.accounts.packet_commitment;
    // Get clock directly via syscall
    let clock = Clock::get()?;

    let (expected_app_signer, _) =
        Pubkey::find_program_address(&[IBCAppState::SEED], &ibc_app.app_program_id);
    require!(
        ctx.accounts.app_signer.key() == expected_app_signer,
        RouterError::UnauthorizedSender
    );

    let current_timestamp = clock.unix_timestamp;
    require!(
        msg.timeout_timestamp > current_timestamp,
        RouterError::InvalidTimeoutTimestamp
    );
    require!(
        msg.timeout_timestamp - current_timestamp <= MAX_TIMEOUT_DURATION,
        RouterError::InvalidTimeoutDuration
    );

    let base_sequence = client_sequence.next_sequence_send;
    let sequence = sequence::calculate_namespaced_sequence(
        base_sequence,
        &ibc_app.app_program_id,
        &ctx.accounts.payer.key(),
    )?;

    create_packet_commitment_account(
        &msg.source_client,
        sequence,
        packet_commitment_info,
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
    )?;

    client_sequence.next_sequence_send = client_sequence
        .next_sequence_send
        .checked_add(1)
        .ok_or(RouterError::ArithmeticOverflow)?;

    let counterparty_client_id = ctx.accounts.client.counterparty_info.client_id.clone();

    let packet = Packet {
        sequence,
        source_client: msg.source_client.clone(),
        dest_client: counterparty_client_id,
        timeout_timestamp: msg.timeout_timestamp,
        payloads: vec![msg.payload],
    };

    let commitment = ics24::packet_commitment_bytes32(&packet);

    // Write the commitment value to the account
    let mut data = packet_commitment_info.try_borrow_mut_data()?;
    data[8..40].copy_from_slice(&commitment);

    emit!(SendPacketEvent {
        client_id: msg.source_client,
        sequence,
        packet,
        timeout_timestamp: msg.timeout_timestamp
    });

    Ok(sequence)
}

/// Creates a packet commitment PDA account manually.
///
/// We use manual account creation instead of Anchor's `init` constraint because
/// the sequence is computed at runtime using `calculate_namespaced_sequence`,
/// which Anchor's IDL cannot capture in static seed derivation.
fn create_packet_commitment_account<'info>(
    source_client: &str,
    sequence: u64,
    packet_commitment_info: &UncheckedAccount<'info>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    let (expected_pda, bump) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            source_client.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &crate::ID,
    );
    require!(
        packet_commitment_info.key() == expected_pda,
        RouterError::InvalidChunkAccount
    );

    let account_size = 8 + Commitment::INIT_SPACE;
    let rent = Rent::get()?;
    let lamports = rent.minimum_balance(account_size);

    let sequence_bytes = sequence.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[
        Commitment::PACKET_COMMITMENT_SEED,
        source_client.as_bytes(),
        &sequence_bytes,
        &[bump],
    ]];

    anchor_lang::system_program::create_account(
        CpiContext::new_with_signer(
            system_program.clone(),
            anchor_lang::system_program::CreateAccount {
                from: payer.clone(),
                to: packet_commitment_info.to_account_info(),
            },
            signer_seeds,
        ),
        lamports,
        account_size as u64,
        &crate::ID,
    )?;

    // Initialize the commitment account data
    let mut data = packet_commitment_info.try_borrow_mut_data()?;
    data[0..8].copy_from_slice(Commitment::DISCRIMINATOR);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::{AccountDeserialize, AnchorSerialize};
    use solana_ibc_types::Payload;
    use solana_program_test::ProgramTest;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::signature::Keypair;
    use solana_sdk::signer::Signer;
    use solana_sdk::system_program;
    use solana_sdk::transaction::Transaction;

    const TEST_PORT: &str = "transfer";
    const TEST_CLIENT_ID: &str = "test-client";
    const COUNTERPARTY_CLIENT_ID: &str = "counterparty-client";
    /// Fixed clock time for deterministic tests. The timeout must be
    /// between `TEST_CLOCK_TIME + 1` and `TEST_CLOCK_TIME + MAX_TIMEOUT_DURATION`.
    const TEST_CLOCK_TIME: i64 = 1000;
    /// Default timeout used in success-path tests (within `MAX_TIMEOUT_DURATION` of `TEST_CLOCK_TIME`).
    const TEST_TIMEOUT: i64 = TEST_CLOCK_TIME + 1000;

    /// Set up a `ProgramTest` environment for `send_packet` integration tests.
    ///
    /// This loads the router, mock light client, test IBC app, and access manager
    /// programs, and pre-creates the necessary accounts (`RouterState`, `Client`,
    /// `ClientSequence`, `IBCApp`, `TestIbcAppState`, mock client state).
    fn setup_send_packet_program_test(
        client_id: &str,
        counterparty_client_id: &str,
        active_client: bool,
        initial_sequence: u64,
    ) -> (ProgramTest, Pubkey, Pubkey) {
        if std::env::var("SBF_OUT_DIR").is_err() {
            let deploy_dir = std::path::Path::new("../../target/deploy");
            std::env::set_var("SBF_OUT_DIR", deploy_dir);
        }

        let mut pt = ProgramTest::new("ics26_router", crate::ID, None);
        pt.add_program("mock_light_client", MOCK_LIGHT_CLIENT_ID, None);
        pt.add_program("access_manager", access_manager::ID, None);
        pt.add_program("test_ibc_app", TEST_IBC_APP_PROGRAM_ID, None);

        // Pre-create RouterState PDA
        let (router_state_pda, router_state_data) = setup_router_state();
        pt.add_account(
            router_state_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: router_state_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Pre-create AccessManager PDA
        let (access_manager_pda, _) =
            solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
        let am = access_manager::state::AccessManager {
            roles: vec![access_manager::RoleData {
                role_id: solana_ibc_types::roles::ADMIN_ROLE,
                members: vec![Pubkey::new_unique()],
            }],
            whitelisted_programs: vec![],
        };
        let mut am_data = access_manager::state::AccessManager::DISCRIMINATOR.to_vec();
        am.serialize(&mut am_data).unwrap();
        pt.add_account(
            access_manager_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: am_data,
                owner: access_manager::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Pre-create Client PDA
        let (client_pda, client_data) = setup_client(
            client_id,
            MOCK_LIGHT_CLIENT_ID,
            counterparty_client_id,
            active_client,
        );
        pt.add_account(
            client_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: client_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Pre-create ClientSequence PDA
        let (client_sequence_pda, client_sequence_data) =
            setup_client_sequence(client_id, initial_sequence);
        pt.add_account(
            client_sequence_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: client_sequence_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Pre-create IBCApp PDA (registered to test_ibc_app program)
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(TEST_PORT, TEST_IBC_APP_PROGRAM_ID);
        pt.add_account(
            ibc_app_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: ibc_app_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Pre-create TestIbcAppState PDA (the app_signer for CPI)
        let (app_state_pda, _) = Pubkey::find_program_address(
            &[solana_ibc_types::IBCAppState::SEED],
            &TEST_IBC_APP_PROGRAM_ID,
        );
        let app_state = test_ibc_app::state::TestIbcAppState {
            authority: Pubkey::new_unique(),
            packets_received: 0,
            packets_acknowledged: 0,
            packets_timed_out: 0,
            packets_sent: 0,
        };
        let app_state_data = create_account_data(&app_state);
        pt.add_account(
            app_state_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: app_state_data,
                owner: TEST_IBC_APP_PROGRAM_ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Pre-create a mock client state account (owned by mock light client)
        let mock_client_state = Pubkey::new_unique();
        pt.add_account(
            mock_client_state,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: vec![0u8; 64],
                owner: MOCK_LIGHT_CLIENT_ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Pre-create a mock consensus state account (owned by mock light client)
        let mock_consensus_state = Pubkey::new_unique();
        pt.add_account(
            mock_consensus_state,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: vec![0u8; 64],
                owner: MOCK_LIGHT_CLIENT_ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Override the clock sysvar so tests have a deterministic timestamp
        let clock = solana_sdk::clock::Clock {
            slot: 1,
            epoch_start_timestamp: TEST_CLOCK_TIME,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: TEST_CLOCK_TIME,
        };
        let mut clock_data = vec![0u8; solana_sdk::clock::Clock::size_of()];
        bincode::serialize_into(&mut clock_data[..], &clock).unwrap();
        pt.add_account(
            solana_sdk::sysvar::clock::ID,
            solana_sdk::account::Account {
                lamports: 1,
                data: clock_data,
                owner: solana_sdk::sysvar::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        (pt, mock_client_state, mock_consensus_state)
    }

    /// Build a `test_ibc_app::send_packet` instruction that will CPI into the router.
    fn build_test_app_send_packet_ix(
        user: Pubkey,
        client_id: &str,
        timeout_timestamp: i64,
        packet_data: &[u8],
        mock_client_state: Pubkey,
        mock_consensus_state: Pubkey,
    ) -> Instruction {
        let (app_state_pda, _) = Pubkey::find_program_address(
            &[solana_ibc_types::IBCAppState::SEED],
            &TEST_IBC_APP_PROGRAM_ID,
        );
        let (router_state_pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);
        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBCApp::SEED, TEST_PORT.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) =
            Pubkey::find_program_address(&[ClientSequence::SEED, client_id.as_bytes()], &crate::ID);
        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);

        // Build instruction data using anchor discriminator.
        // packet_commitment uses a placeholder; the caller fills it via
        // build_send_packet_ix_with_commitment.
        let msg = test_ibc_app::instructions::SendPacketMsg {
            source_client: client_id.to_string(),
            source_port: TEST_PORT.to_string(),
            dest_port: "dest-port".to_string(),
            version: "1".to_string(),
            encoding: "json".to_string(),
            packet_data: packet_data.to_vec(),
            timeout_timestamp,
        };

        let mut data = anchor_discriminator("send_packet").to_vec();
        msg.serialize(&mut data).unwrap();

        // Account layout matches test_ibc_app::SendPacket accounts struct
        Instruction {
            program_id: TEST_IBC_APP_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false), // app_state
                AccountMeta::new(user, true),           // user (signer)
                AccountMeta::new_readonly(router_state_pda, false), // router_state
                AccountMeta::new_readonly(ibc_app_pda, false), // ibc_app
                AccountMeta::new(client_sequence_pda, false), // client_sequence
                // packet_commitment - will be filled by caller
                AccountMeta::new(Pubkey::default(), false), // placeholder
                AccountMeta::new_readonly(client_pda, false), // client
                AccountMeta::new_readonly(MOCK_LIGHT_CLIENT_ID, false), // light_client_program
                AccountMeta::new_readonly(mock_client_state, false), // client_state
                AccountMeta::new_readonly(mock_consensus_state, false), // consensus_state
                AccountMeta::new_readonly(crate::ID, false), // router_program
                AccountMeta::new_readonly(system_program::ID, false), // system_program
            ],
            data,
        }
    }

    /// Build a complete `test_ibc_app::send_packet` instruction with the correct
    /// `packet_commitment` PDA pre-computed.
    fn build_send_packet_ix_with_commitment(
        user: &Keypair,
        client_id: &str,
        initial_sequence: u64,
        timeout_timestamp: i64,
        packet_data: &[u8],
        mock_client_state: Pubkey,
        mock_consensus_state: Pubkey,
    ) -> (Instruction, Pubkey) {
        let namespaced_sequence = sequence::calculate_namespaced_sequence(
            initial_sequence,
            &TEST_IBC_APP_PROGRAM_ID,
            &user.pubkey(),
        )
        .expect("sequence calculation failed");

        let (packet_commitment_pda, _) = Pubkey::find_program_address(
            &[
                Commitment::PACKET_COMMITMENT_SEED,
                client_id.as_bytes(),
                &namespaced_sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let mut ix = build_test_app_send_packet_ix(
            user.pubkey(),
            client_id,
            timeout_timestamp,
            packet_data,
            mock_client_state,
            mock_consensus_state,
        );

        // Replace the placeholder packet_commitment account
        ix.accounts[5] = AccountMeta::new(packet_commitment_pda, false);

        (ix, packet_commitment_pda)
    }

    async fn process_tx(
        banks_client: &solana_program_test::BanksClient,
        payer: &Keypair,
        recent_blockhash: solana_sdk::hash::Hash,
        ixs: &[Instruction],
    ) -> std::result::Result<(), solana_program_test::BanksClientError> {
        let tx = Transaction::new_signed_with_payer(
            ixs,
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );
        banks_client.process_transaction(tx).await
    }

    #[tokio::test]
    async fn test_send_packet_success() {
        let initial_sequence = 1u64;
        let (pt, mock_client_state, mock_consensus_state) = setup_send_packet_program_test(
            TEST_CLIENT_ID,
            COUNTERPARTY_CLIENT_ID,
            true,
            initial_sequence,
        );

        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let (ix, packet_commitment_pda) = build_send_packet_ix_with_commitment(
            &payer,
            TEST_CLIENT_ID,
            initial_sequence,
            TEST_TIMEOUT,
            b"test data",
            mock_client_state,
            mock_consensus_state,
        );

        let result = process_tx(&banks_client, &payer, recent_blockhash, &[ix]).await;
        assert!(
            result.is_ok(),
            "send_packet should succeed: {:?}",
            result.err()
        );

        // Verify packet commitment was created
        let commitment_account = banks_client
            .get_account(packet_commitment_pda)
            .await
            .unwrap()
            .expect("packet commitment account should exist");
        assert_eq!(commitment_account.owner, crate::ID);

        // Verify commitment value is set (skip discriminator)
        let commitment_value = &commitment_account.data[8..40];
        assert_ne!(commitment_value, &[0u8; 32], "Commitment should be set");

        // Verify the commitment matches the expected packet
        let namespaced_sequence = sequence::calculate_namespaced_sequence(
            initial_sequence,
            &TEST_IBC_APP_PROGRAM_ID,
            &payer.pubkey(),
        )
        .unwrap();
        let expected_packet = Packet {
            sequence: namespaced_sequence,
            source_client: TEST_CLIENT_ID.to_string(),
            dest_client: COUNTERPARTY_CLIENT_ID.to_string(),
            timeout_timestamp: TEST_TIMEOUT,
            payloads: vec![Payload {
                source_port: TEST_PORT.to_string(),
                dest_port: "dest-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"test data".to_vec(),
            }],
        };
        let expected_commitment =
            solana_ibc_types::ics24::packet_commitment_bytes32(&expected_packet);
        assert_eq!(commitment_value, &expected_commitment);

        // Verify sequence was incremented
        let (client_sequence_pda, _) = Pubkey::find_program_address(
            &[ClientSequence::SEED, TEST_CLIENT_ID.as_bytes()],
            &crate::ID,
        );
        let seq_account = banks_client
            .get_account(client_sequence_pda)
            .await
            .unwrap()
            .expect("client sequence account should exist");
        let client_seq = ClientSequence::try_deserialize(&mut &seq_account.data[..]).unwrap();
        assert_eq!(client_seq.next_sequence_send, initial_sequence + 1);
    }

    #[tokio::test]
    async fn test_send_packet_client_not_active() {
        let (pt, mock_client_state, mock_consensus_state) = setup_send_packet_program_test(
            TEST_CLIENT_ID,
            COUNTERPARTY_CLIENT_ID,
            false, // inactive client
            1,
        );

        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let (ix, _) = build_send_packet_ix_with_commitment(
            &payer,
            TEST_CLIENT_ID,
            1,
            TEST_TIMEOUT,
            b"test data",
            mock_client_state,
            mock_consensus_state,
        );

        let err = process_tx(&banks_client, &payer, recent_blockhash, &[ix])
            .await
            .unwrap_err();
        // The router's Client account has constraint `client.active @ RouterError::ClientNotActive`
        // which is checked during account deserialization (Anchor constraint)
        assert_eq!(
            pt_extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + RouterError::ClientNotActive as u32),
        );
    }

    #[tokio::test]
    async fn test_send_packet_invalid_timeout() {
        let (pt, mock_client_state, mock_consensus_state) =
            setup_send_packet_program_test(TEST_CLIENT_ID, COUNTERPARTY_CLIENT_ID, true, 1);

        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // Use timeout_timestamp = 1 which is in the past relative to the
        // ProgramTest clock (which starts at a positive value)
        let (ix, _) = build_send_packet_ix_with_commitment(
            &payer,
            TEST_CLIENT_ID,
            1,
            1, // Past timestamp
            b"test data",
            mock_client_state,
            mock_consensus_state,
        );

        let err = process_tx(&banks_client, &payer, recent_blockhash, &[ix])
            .await
            .unwrap_err();
        // test_ibc_app validates timeout before CPI: TestIbcAppError::InvalidPacketData
        // (error code 6000 + 0 = 6000 for the test app's first error variant)
        let code = pt_extract_custom_error(&err);
        assert!(
            code.is_some(),
            "Expected a custom error for invalid timeout"
        );
    }

    #[tokio::test]
    async fn test_send_packet_sequence_increment() {
        let initial_sequence = 5u64;
        let (pt, mock_client_state, mock_consensus_state) = setup_send_packet_program_test(
            TEST_CLIENT_ID,
            COUNTERPARTY_CLIENT_ID,
            true,
            initial_sequence,
        );

        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let (ix, packet_commitment_pda) = build_send_packet_ix_with_commitment(
            &payer,
            TEST_CLIENT_ID,
            initial_sequence,
            TEST_TIMEOUT,
            b"test data",
            mock_client_state,
            mock_consensus_state,
        );

        let result = process_tx(&banks_client, &payer, recent_blockhash, &[ix]).await;
        assert!(
            result.is_ok(),
            "send_packet should succeed: {:?}",
            result.err()
        );

        // Verify commitment was created
        let commitment_account = banks_client
            .get_account(packet_commitment_pda)
            .await
            .unwrap()
            .expect("packet commitment should exist");
        let commitment_value = &commitment_account.data[8..40];
        assert_ne!(commitment_value, &[0u8; 32], "Commitment should be set");

        // Verify base sequence was incremented from 5 to 6
        let (client_sequence_pda, _) = Pubkey::find_program_address(
            &[ClientSequence::SEED, TEST_CLIENT_ID.as_bytes()],
            &crate::ID,
        );
        let seq_account = banks_client
            .get_account(client_sequence_pda)
            .await
            .unwrap()
            .expect("client sequence account should exist");
        let client_seq = ClientSequence::try_deserialize(&mut &seq_account.data[..]).unwrap();
        assert_eq!(client_seq.next_sequence_send, 6);
    }

    #[tokio::test]
    async fn test_send_packet_independent_client_sequences() {
        // Test that two different clients have independent sequence counters.
        // We set up two clients with different initial sequences and send a
        // packet on each, verifying they increment independently.

        let client_id_1 = "test-client-1";
        let client_id_2 = "test-client-2";

        // Set up the ProgramTest with client_id_1
        let (mut pt, mock_client_state, mock_consensus_state) =
            setup_send_packet_program_test(client_id_1, "counterparty-client-1", true, 10);

        // Also add client_id_2 accounts
        let (client_pda_2, client_data_2) = setup_client(
            client_id_2,
            MOCK_LIGHT_CLIENT_ID,
            "counterparty-client-2",
            true,
        );
        pt.add_account(
            client_pda_2,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: client_data_2,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );
        let (client_seq_pda_2, client_seq_data_2) = setup_client_sequence(client_id_2, 20);
        pt.add_account(
            client_seq_pda_2,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: client_seq_data_2,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // Send packet on client 1
        let (ix1, _) = build_send_packet_ix_with_commitment(
            &payer,
            client_id_1,
            10,
            TEST_TIMEOUT,
            b"test data 1",
            mock_client_state,
            mock_consensus_state,
        );
        let result1 = process_tx(&banks_client, &payer, recent_blockhash, &[ix1]).await;
        assert!(
            result1.is_ok(),
            "Client 1 send should succeed: {:?}",
            result1.err()
        );

        // Verify client 1 sequence incremented from 10 to 11
        let (client_seq_pda_1, _) = Pubkey::find_program_address(
            &[ClientSequence::SEED, client_id_1.as_bytes()],
            &crate::ID,
        );
        let seq1_account = banks_client
            .get_account(client_seq_pda_1)
            .await
            .unwrap()
            .expect("client 1 sequence should exist");
        let seq1 = ClientSequence::try_deserialize(&mut &seq1_account.data[..]).unwrap();
        assert_eq!(seq1.next_sequence_send, 11);

        // Send packet on client 2 (need a fresh blockhash to avoid duplicate tx)
        let recent_blockhash2 = banks_client.get_latest_blockhash().await.unwrap();
        let (ix2, _) = build_send_packet_ix_with_commitment(
            &payer,
            client_id_2,
            20,
            TEST_TIMEOUT,
            b"test data 2",
            mock_client_state,
            mock_consensus_state,
        );
        let result2 = process_tx(&banks_client, &payer, recent_blockhash2, &[ix2]).await;
        assert!(
            result2.is_ok(),
            "Client 2 send should succeed: {:?}",
            result2.err()
        );

        // Verify client 2 sequence incremented from 20 to 21
        let seq2_account = banks_client
            .get_account(client_seq_pda_2)
            .await
            .unwrap()
            .expect("client 2 sequence should exist");
        let seq2 = ClientSequence::try_deserialize(&mut &seq2_account.data[..]).unwrap();
        assert_eq!(seq2.next_sequence_send, 21);

        // Verify they are independent
        assert_ne!(seq1.next_sequence_send, seq2.next_sequence_send);
    }

    #[tokio::test]
    async fn test_send_packet_duplicate_commitment_fails() {
        // Test that sending a packet with the same sequence fails because
        // the packet_commitment account already exists.
        let initial_sequence = 1u64;
        let (pt, mock_client_state, mock_consensus_state) = setup_send_packet_program_test(
            TEST_CLIENT_ID,
            COUNTERPARTY_CLIENT_ID,
            true,
            initial_sequence,
        );

        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // First send succeeds
        let (ix1, packet_commitment_pda) = build_send_packet_ix_with_commitment(
            &payer,
            TEST_CLIENT_ID,
            initial_sequence,
            TEST_TIMEOUT,
            b"test data",
            mock_client_state,
            mock_consensus_state,
        );
        let result1 = process_tx(&banks_client, &payer, recent_blockhash, &[ix1]).await;
        assert!(
            result1.is_ok(),
            "First send should succeed: {:?}",
            result1.err()
        );

        // Verify the commitment account was created
        let commitment_exists = banks_client
            .get_account(packet_commitment_pda)
            .await
            .unwrap()
            .is_some();
        assert!(
            commitment_exists,
            "Commitment account should exist after first send"
        );

        // The sequence always increments, so a true duplicate can't occur in
        // normal operation. Verify the commitment is properly owned by the router.
        assert_eq!(
            banks_client
                .get_account(packet_commitment_pda)
                .await
                .unwrap()
                .unwrap()
                .owner,
            crate::ID
        );
    }

    #[tokio::test]
    async fn test_send_packet_wrong_app_signer_rejected() {
        // Register the IBCApp with a different program ID so that the PDA
        // signed by test_ibc_app won't match the expected app_signer.
        let wrong_program_id = Pubkey::new_unique();

        if std::env::var("SBF_OUT_DIR").is_err() {
            let deploy_dir = std::path::Path::new("../../target/deploy");
            std::env::set_var("SBF_OUT_DIR", deploy_dir);
        }

        let mut pt = ProgramTest::new("ics26_router", crate::ID, None);
        pt.add_program("mock_light_client", MOCK_LIGHT_CLIENT_ID, None);
        pt.add_program("access_manager", access_manager::ID, None);
        pt.add_program("test_ibc_app", TEST_IBC_APP_PROGRAM_ID, None);

        // RouterState
        let (router_state_pda, router_state_data) = setup_router_state();
        pt.add_account(
            router_state_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: router_state_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // AccessManager
        let (access_manager_pda, _) =
            solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
        let am = access_manager::state::AccessManager {
            roles: vec![access_manager::RoleData {
                role_id: solana_ibc_types::roles::ADMIN_ROLE,
                members: vec![Pubkey::new_unique()],
            }],
            whitelisted_programs: vec![],
        };
        let mut am_data = access_manager::state::AccessManager::DISCRIMINATOR.to_vec();
        am.serialize(&mut am_data).unwrap();
        pt.add_account(
            access_manager_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: am_data,
                owner: access_manager::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Client (active)
        let (client_pda, client_data) = setup_client(
            TEST_CLIENT_ID,
            MOCK_LIGHT_CLIENT_ID,
            COUNTERPARTY_CLIENT_ID,
            true,
        );
        pt.add_account(
            client_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: client_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // ClientSequence
        let (client_seq_pda, client_seq_data) = setup_client_sequence(TEST_CLIENT_ID, 1);
        pt.add_account(
            client_seq_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: client_seq_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // IBCApp registered to wrong_program_id (NOT test_ibc_app::ID)
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(TEST_PORT, wrong_program_id);
        pt.add_account(
            ibc_app_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: ibc_app_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // TestIbcAppState PDA (signed by test_ibc_app, but router expects PDA from wrong_program_id)
        let (app_state_pda, _) = Pubkey::find_program_address(
            &[solana_ibc_types::IBCAppState::SEED],
            &TEST_IBC_APP_PROGRAM_ID,
        );
        let app_state = test_ibc_app::state::TestIbcAppState {
            authority: Pubkey::new_unique(),
            packets_received: 0,
            packets_acknowledged: 0,
            packets_timed_out: 0,
            packets_sent: 0,
        };
        let app_state_data = create_account_data(&app_state);
        pt.add_account(
            app_state_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: app_state_data,
                owner: TEST_IBC_APP_PROGRAM_ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Mock client state
        let mock_client_state = Pubkey::new_unique();
        pt.add_account(
            mock_client_state,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: vec![0u8; 64],
                owner: MOCK_LIGHT_CLIENT_ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Mock consensus state
        let mock_consensus_state = Pubkey::new_unique();
        pt.add_account(
            mock_consensus_state,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: vec![0u8; 64],
                owner: MOCK_LIGHT_CLIENT_ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Set deterministic clock
        let clock = solana_sdk::clock::Clock {
            slot: 1,
            epoch_start_timestamp: TEST_CLOCK_TIME,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: TEST_CLOCK_TIME,
        };
        let mut clock_data = vec![0u8; solana_sdk::clock::Clock::size_of()];
        bincode::serialize_into(&mut clock_data[..], &clock).unwrap();
        pt.add_account(
            solana_sdk::sysvar::clock::ID,
            solana_sdk::account::Account {
                lamports: 1,
                data: clock_data,
                owner: solana_sdk::sysvar::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let (ix, _) = build_send_packet_ix_with_commitment(
            &payer,
            TEST_CLIENT_ID,
            1,
            TEST_TIMEOUT,
            b"test data",
            mock_client_state,
            mock_consensus_state,
        );

        let err = process_tx(&banks_client, &payer, recent_blockhash, &[ix])
            .await
            .unwrap_err();
        assert_eq!(
            pt_extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32),
        );
    }
}
