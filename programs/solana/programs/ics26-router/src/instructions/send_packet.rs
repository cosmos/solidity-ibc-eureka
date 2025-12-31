use crate::errors::RouterError;
use crate::events::PacketSent;
use crate::state::*;
use crate::utils::ics24;
use crate::utils::sequence;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgSendPacket)]
pub struct SendPacket<'info> {
    #[account(
        seeds = [RouterState::SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [IBCApp::SEED, msg.payload.source_port.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    #[account(
        mut,
        seeds = [ClientSequence::SEED, msg.source_client.as_bytes()],
        bump
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    /// Packet commitment account - manually created with runtime-calculated sequence
    /// CHECK: Manually validated and created in instruction handler
    #[account(mut)]
    pub packet_commitment: UncheckedAccount<'info>,

    /// Instructions sysvar for validating CPI caller
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    /// Allow payer to be separate from IBC app
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,

    #[account(
        seeds = [Client::SEED, msg.source_client.as_bytes()],
        bump,
        constraint = client.active @ RouterError::ClientNotActive,
    )]
    pub client: Account<'info, Client>,
}

pub fn send_packet(ctx: Context<SendPacket>, msg: MsgSendPacket) -> Result<u64> {
    let ibc_app = &ctx.accounts.ibc_app;
    let client_sequence = &mut ctx.accounts.client_sequence;
    let packet_commitment_info = &ctx.accounts.packet_commitment;
    // Get clock directly via syscall
    let clock = Clock::get()?;

    solana_ibc_types::validate_cpi_caller(
        &ctx.accounts.instruction_sysvar,
        &ibc_app.app_program_id,
        &crate::ID,
    )
    .map_err(RouterError::from)?;

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

    emit!(PacketSent {
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
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_ibc_types::Payload;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{clock::Clock, system_program};

    struct SendPacketTestContext {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
        packet_commitment_pubkey: Pubkey,
        client_sequence_pubkey: Pubkey,
        sequence: u64,      // The namespaced sequence number
        base_sequence: u64, // The base sequence (before namespacing)
    }

    struct SendPacketTestParams {
        client_id: &'static str,
        port_id: &'static str,
        app_program_id: Option<Pubkey>,
        cpi_caller_program_id: Pubkey,
        active_client: bool,
        current_timestamp: i64,
        timeout_timestamp: i64,
        initial_sequence: u64,
    }

    impl Default for SendPacketTestParams {
        fn default() -> Self {
            let app_program_id = Pubkey::new_unique();
            Self {
                client_id: "test-client",
                port_id: "test-port",
                app_program_id: Some(app_program_id),
                cpi_caller_program_id: app_program_id,
                active_client: true,
                current_timestamp: 1000,
                timeout_timestamp: 2000,
                initial_sequence: 1, // IBC sequences start from 1
            }
        }
    }

    fn setup_send_packet_test_with_params(params: SendPacketTestParams) -> SendPacketTestContext {
        let app_program_id = params.app_program_id.unwrap_or_else(Pubkey::new_unique);
        let payer = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();
        let (client_pda, client_data) = setup_client(
            params.client_id,
            Pubkey::new_unique(),
            "counterparty-client",
            params.active_client,
        );
        let (client_sequence_pda, client_sequence_data) =
            setup_client_sequence(params.client_id, params.initial_sequence);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(params.port_id, app_program_id);

        let msg = MsgSendPacket {
            source_client: params.client_id.to_string(),
            timeout_timestamp: params.timeout_timestamp,
            payload: Payload {
                source_port: params.port_id.to_string(),
                dest_port: "dest-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"test data".to_vec(),
            },
        };

        // Calculate the namespaced sequence using the same logic as the instruction
        let namespaced_sequence = sequence::calculate_namespaced_sequence(
            params.initial_sequence,
            &app_program_id,
            &payer,
        )
        .expect("sequence calculation failed");

        let (packet_commitment_pda, _) = Pubkey::find_program_address(
            &[
                Commitment::PACKET_COMMITMENT_SEED,
                msg.source_client.as_bytes(),
                &namespaced_sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_commitment_pda, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(client_pda, false),
            ],
            data: crate::instruction::SendPacket { msg }.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda),
            create_instructions_sysvar_account_with_caller(params.cpi_caller_program_id),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_account(client_pda, client_data, crate::ID),
        ];

        SendPacketTestContext {
            instruction,
            accounts,
            packet_commitment_pubkey: packet_commitment_pda,
            client_sequence_pubkey: client_sequence_pda,
            sequence: namespaced_sequence,
            base_sequence: params.initial_sequence,
        }
    }

    fn create_clock_data(timestamp: i64) -> Vec<u8> {
        let mut clock_data = vec![0u8; Clock::size_of()];
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: timestamp,
        };
        bincode::serialize_into(&mut clock_data[..], &clock).unwrap();
        clock_data
    }

    #[test]
    fn test_send_packet_success() {
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams::default());

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // Calculate expected rent-exempt lamports for Commitment account
        let commitment_rent = {
            use anchor_lang::Space;
            use solana_sdk::rent::Rent;
            let account_size = 8 + Commitment::INIT_SPACE;
            Rent::default().minimum_balance(account_size)
        };

        let checks = vec![
            Check::success(),
            Check::account(&ctx.packet_commitment_pubkey)
                .lamports(commitment_rent)
                .owner(&crate::ID)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);

        let result = mollusk.process_instruction(&ctx.instruction, &ctx.accounts);

        // Check packet commitment
        let expected_packet = Packet {
            sequence: ctx.sequence,
            source_client: "test-client".to_string(),
            dest_client: "counterparty-client".to_string(),
            timeout_timestamp: 2000,
            payloads: vec![Payload {
                source_port: "test-port".to_string(),
                dest_port: "dest-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"test data".to_vec(),
            }],
        };
        let expected_commitment = crate::utils::ics24::packet_commitment_bytes32(&expected_packet);
        let commitment_data = get_account_data_from_mollusk(&result, &ctx.packet_commitment_pubkey)
            .expect("packet commitment account not found");
        assert_eq!(commitment_data[..32], expected_commitment);

        // Use the more reliable pubkey-based lookup
        let next_sequence =
            get_client_sequence_from_result_by_pubkey(&result, &ctx.client_sequence_pubkey)
                .expect("client_sequence not found");
        assert_eq!(next_sequence, ctx.base_sequence + 1); // Should be incremented by 1
    }

    #[test]
    fn test_send_packet_direct_call_rejected() {
        // Test that direct calls (not via CPI) are rejected
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            cpi_caller_program_id: crate::ID,
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::DirectCallNotAllowed as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_unauthorized_app_caller() {
        // Test that CPI from unauthorized program is rejected
        let unauthorized_program = Pubkey::new_unique();
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            cpi_caller_program_id: unauthorized_program,
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_fake_sysvar_wormhole_attack() {
        // Test that Wormhole-style fake sysvar attacks are rejected
        let app_program_id = Pubkey::new_unique();
        let mut ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            app_program_id: Some(app_program_id),
            cpi_caller_program_id: app_program_id,
            ..Default::default()
        });

        // Simulate Wormhole attack: replace real sysvar with a completely different account
        let (fake_sysvar_pubkey, fake_sysvar_account) =
            create_fake_instructions_sysvar_account(app_program_id);

        // Modify the instruction to reference the fake sysvar (simulating attacker control)
        ctx.instruction.accounts[4] = AccountMeta::new_readonly(fake_sysvar_pubkey, false);
        ctx.accounts[4] = (fake_sysvar_pubkey, fake_sysvar_account);

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // Should be rejected by Anchor's address constraint check
        let checks = vec![Check::err(ProgramError::Custom(
            anchor_lang::error::ErrorCode::ConstraintAddress as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_client_not_active() {
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            active_client: false,
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ClientNotActive as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_invalid_timeout() {
        let mut ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            current_timestamp: 1000,
            timeout_timestamp: 900, // Past timestamp
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // Add Clock sysvar with current timestamp (1000) - packet timeout is 900 (expired)
        let clock_data = create_clock_data(1000);
        ctx.accounts
            .push(create_clock_account_with_data(clock_data));

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidTimeoutTimestamp as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_timeout_duration_too_long() {
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            current_timestamp: 1000,
            timeout_timestamp: 1000 + MAX_TIMEOUT_DURATION + 1, // Too far in future
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidTimeoutDuration as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_sequence_increment() {
        // Test that sending multiple packets increments the sequence correctly
        let params = SendPacketTestParams {
            initial_sequence: 5,
            ..Default::default()
        };
        let ctx = setup_send_packet_test_with_params(params);

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let result = mollusk.process_instruction(&ctx.instruction, &ctx.accounts);

        // Check that packet was created with sequence 5
        let commitment_data = get_account_data_from_mollusk(&result, &ctx.packet_commitment_pubkey)
            .expect("packet commitment account not found");
        assert_ne!(commitment_data[..32], [0u8; 32]); // Commitment should be set

        // Check base sequence was incremented from 5 to 6
        let next_sequence =
            get_client_sequence_from_result_by_pubkey(&result, &ctx.client_sequence_pubkey)
                .expect("client_sequence not found");
        assert_eq!(next_sequence, 6);
    }

    #[test]
    fn test_send_packet_independent_client_sequences() {
        // Test that two different clients have independent sequence counters
        let app_program_id = Pubkey::new_unique();
        let port_id = "test-port";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program_id);

        // Create first client with sequence 10
        let client_id_1 = "test-client-1";
        let (client_pda_1, client_data_1) = setup_client(
            client_id_1,
            Pubkey::new_unique(),
            "counterparty-client-1",
            true,
        );
        let (client_sequence_pda_1, client_sequence_data_1) =
            setup_client_sequence(client_id_1, 10);

        // Create second client with sequence 20
        let client_id_2 = "test-client-2";
        let (client_pda_2, client_data_2) = setup_client(
            client_id_2,
            Pubkey::new_unique(),
            "counterparty-client-2",
            true,
        );
        let (client_sequence_pda_2, client_sequence_data_2) =
            setup_client_sequence(client_id_2, 20);

        // Test sending packet on client 1
        let msg_1 = MsgSendPacket {
            source_client: client_id_1.to_string(),
            timeout_timestamp: 2000,
            payload: Payload {
                source_port: port_id.to_string(),
                dest_port: "dest-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"test data 1".to_vec(),
            },
        };

        let payer = Pubkey::new_unique();

        // Calculate namespaced sequence for client 1
        let namespaced_seq_1 = sequence::calculate_namespaced_sequence(10, &app_program_id, &payer)
            .expect("sequence calculation failed");

        let (packet_commitment_pda_1, _) = Pubkey::find_program_address(
            &[
                Commitment::PACKET_COMMITMENT_SEED,
                msg_1.source_client.as_bytes(),
                &namespaced_seq_1.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_1 = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda_1, false),
                AccountMeta::new(packet_commitment_pda_1, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(client_pda_1, false),
            ],
            data: crate::instruction::SendPacket { msg: msg_1 }.data(),
        };

        let accounts_1 = vec![
            create_account(router_state_pda, router_state_data.clone(), crate::ID),
            create_account(ibc_app_pda, ibc_app_data.clone(), crate::ID),
            create_account(client_sequence_pda_1, client_sequence_data_1, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda_1),
            create_instructions_sysvar_account_with_caller(app_program_id),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_account(client_pda_1, client_data_1, crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());
        let result_1 = mollusk.process_instruction(&instruction_1, &accounts_1);

        // Verify client 1 sequence was incremented from 10 to 11
        let client_1_sequence =
            get_client_sequence_from_result_by_pubkey(&result_1, &client_sequence_pda_1)
                .expect("client_1_sequence not found");
        assert_eq!(client_1_sequence, 11);

        // Test sending packet on client 2
        let msg_2 = MsgSendPacket {
            source_client: client_id_2.to_string(),
            timeout_timestamp: 2000,
            payload: Payload {
                source_port: port_id.to_string(),
                dest_port: "dest-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"test data 2".to_vec(),
            },
        };

        // Calculate namespaced sequence for client 2
        let namespaced_seq_2 = sequence::calculate_namespaced_sequence(20, &app_program_id, &payer)
            .expect("sequence calculation failed");

        let (packet_commitment_pda_2, _) = Pubkey::find_program_address(
            &[
                Commitment::PACKET_COMMITMENT_SEED,
                msg_2.source_client.as_bytes(),
                &namespaced_seq_2.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_2 = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda_2, false),
                AccountMeta::new(packet_commitment_pda_2, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(client_pda_2, false),
            ],
            data: crate::instruction::SendPacket { msg: msg_2 }.data(),
        };

        let accounts_2 = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_sequence_pda_2, client_sequence_data_2, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda_2),
            create_instructions_sysvar_account_with_caller(app_program_id),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_account(client_pda_2, client_data_2, crate::ID),
        ];

        let result_2 = mollusk.process_instruction(&instruction_2, &accounts_2);

        // Verify client 2 sequence was incremented from 20 to 21
        let client_2_sequence =
            get_client_sequence_from_result_by_pubkey(&result_2, &client_sequence_pda_2)
                .expect("client_2_sequence not found");
        assert_eq!(client_2_sequence, 21);

        // Verify the sequences are independent (client 1 = 11, client 2 = 21)
        assert_ne!(client_1_sequence, client_2_sequence);
    }

    #[test]
    fn test_send_packet_concurrent_different_programs() {
        // Test that two different programs can send packets concurrently with the same base sequence
        // because they get different namespaced sequences
        let app_program_1 = Pubkey::new_unique();
        let app_program_2 = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let client_id = "test-client";
        let port_id_1 = "test-port-1";
        let port_id_2 = "test-port-2";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (client_pda, client_data) =
            setup_client(client_id, Pubkey::new_unique(), "counterparty-client", true);
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 1);
        let (ibc_app_pda_1, ibc_app_data_1) = setup_ibc_app(port_id_1, app_program_1);
        let (ibc_app_pda_2, ibc_app_data_2) = setup_ibc_app(port_id_2, app_program_2);

        // Program 1 sends first
        let msg_1 = MsgSendPacket {
            source_client: client_id.to_string(),
            timeout_timestamp: 2000,
            payload: Payload {
                source_port: port_id_1.to_string(),
                dest_port: "dest-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"program 1 data".to_vec(),
            },
        };

        let namespaced_seq_1 = sequence::calculate_namespaced_sequence(1, &app_program_1, &payer)
            .expect("sequence calculation failed");

        let (packet_commitment_pda_1, _) = Pubkey::find_program_address(
            &[
                Commitment::PACKET_COMMITMENT_SEED,
                msg_1.source_client.as_bytes(),
                &namespaced_seq_1.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_1 = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda_1, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_commitment_pda_1, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(client_pda, false),
            ],
            data: crate::instruction::SendPacket { msg: msg_1 }.data(),
        };

        let accounts_1 = vec![
            create_account(router_state_pda, router_state_data.clone(), crate::ID),
            create_account(ibc_app_pda_1, ibc_app_data_1, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda_1),
            create_instructions_sysvar_account_with_caller(app_program_1),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_account(client_pda, client_data.clone(), crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());
        let result_1 = mollusk.process_instruction(&instruction_1, &accounts_1);
        assert!(
            !result_1.program_result.is_err(),
            "Program 1 should succeed"
        );

        // Get updated client_sequence after first send
        let updated_client_sequence =
            get_client_sequence_from_result_by_pubkey(&result_1, &client_sequence_pda)
                .expect("client_sequence not found");
        assert_eq!(updated_client_sequence, 2); // Should be incremented to 2

        // Program 2 sends with the SAME base sequence (now 2)
        // But different namespaced sequence because different program_id
        let (_, updated_client_sequence_data) =
            setup_client_sequence(client_id, updated_client_sequence);

        let msg_2 = MsgSendPacket {
            source_client: client_id.to_string(),
            timeout_timestamp: 2000,
            payload: Payload {
                source_port: port_id_2.to_string(),
                dest_port: "dest-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"program 2 data".to_vec(),
            },
        };

        let namespaced_seq_2 = sequence::calculate_namespaced_sequence(2, &app_program_2, &payer)
            .expect("sequence calculation failed");

        // Verify the namespaced sequences are different even though base is the same
        // (This will almost certainly be true with high probability)
        assert_ne!(
            namespaced_seq_1, namespaced_seq_2,
            "Different programs should get different namespaced sequences"
        );

        let (packet_commitment_pda_2, _) = Pubkey::find_program_address(
            &[
                Commitment::PACKET_COMMITMENT_SEED,
                msg_2.source_client.as_bytes(),
                &namespaced_seq_2.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_2 = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda_2, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_commitment_pda_2, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(client_pda, false),
            ],
            data: crate::instruction::SendPacket { msg: msg_2 }.data(),
        };

        let accounts_2 = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda_2, ibc_app_data_2, crate::ID),
            create_account(client_sequence_pda, updated_client_sequence_data, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda_2),
            create_instructions_sysvar_account_with_caller(app_program_2),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_account(client_pda, client_data, crate::ID),
        ];

        let result_2 = mollusk.process_instruction(&instruction_2, &accounts_2);
        assert!(
            !result_2.program_result.is_err(),
            "Program 2 should also succeed"
        );

        // Verify both packets were created successfully
        let commitment_1 = get_account_data_from_mollusk(&result_1, &packet_commitment_pda_1)
            .expect("packet 1 commitment not found");
        let commitment_2 = get_account_data_from_mollusk(&result_2, &packet_commitment_pda_2)
            .expect("packet 2 commitment not found");

        assert_ne!(commitment_1[..32], [0u8; 32], "Commitment 1 should be set");
        assert_ne!(commitment_2[..32], [0u8; 32], "Commitment 2 should be set");
        assert_ne!(
            commitment_1[..32],
            commitment_2[..32],
            "Commitments should be different"
        );
    }

    #[test]
    fn test_send_packet_duplicate_commitment_fails() {
        // Test that sending a packet with the same (client_id, sequence) fails
        // because the packet_commitment account already exists
        let params = SendPacketTestParams {
            initial_sequence: 1,
            ..Default::default()
        };
        let mut ctx = setup_send_packet_test_with_params(params);

        // Replace the uninitialized packet_commitment account with an already-initialized one
        // This simulates trying to send a packet that already has a commitment
        let existing_commitment = Commitment {
            value: [1u8; 32], // Some existing commitment value
        };

        let account_size = 8 + Commitment::INIT_SPACE;
        let mut data = vec![0u8; account_size];

        // Add Anchor discriminator
        data[0..8].copy_from_slice(Commitment::DISCRIMINATOR);

        // Serialize the commitment
        let mut cursor = std::io::Cursor::new(&mut data[8..]);
        existing_commitment.serialize(&mut cursor).unwrap();

        // Find and replace the packet_commitment account
        let commitment_index = ctx
            .accounts
            .iter()
            .position(|(pubkey, _)| *pubkey == ctx.packet_commitment_pubkey)
            .unwrap();

        ctx.accounts[commitment_index] = (
            ctx.packet_commitment_pubkey,
            solana_sdk::account::Account {
                lamports: Rent::default().minimum_balance(account_size),
                data,
                owner: crate::ID, // Owned by our program (already initialized)
                executable: false,
                rent_epoch: 0,
            },
        );

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // This should fail because packet_commitment account already exists
        // The `init` constraint will fail with Anchor's "account already in use" error
        let error_checks = vec![Check::err(ProgramError::Custom(0))]; // Anchor error code 0

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &error_checks);
    }
}
