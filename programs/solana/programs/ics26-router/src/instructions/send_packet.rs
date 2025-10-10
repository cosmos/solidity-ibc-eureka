use crate::errors::RouterError;
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use solana_ibc_types::events::SendPacketEvent;

#[derive(Accounts)]
#[instruction(msg: MsgSendPacket)]
pub struct SendPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [IBC_APP_SEED, msg.payload.source_port.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    #[account(
        mut,
        seeds = [CLIENT_SEQUENCE_SEED, msg.source_client.as_bytes()],
        bump
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    #[account(
        init,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE,
        seeds = [
            PACKET_COMMITMENT_SEED,
            msg.source_client.as_bytes(),
            &client_sequence.next_sequence_send.to_le_bytes()
        ],
        bump
    )]
    pub packet_commitment: Account<'info, Commitment>,

    /// The IBC app calling this instruction
    pub app_caller: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,

    pub clock: Sysvar<'info, Clock>,

    #[account(
        seeds = [CLIENT_SEED, msg.source_client.as_bytes()],
        bump,
        constraint = client.active @ RouterError::ClientNotActive,
    )]
    pub client: Account<'info, Client>,
}

pub fn send_packet(ctx: Context<SendPacket>, msg: MsgSendPacket) -> Result<u64> {
    // TODO: Support multi-payload packets #602
    let ibc_app = &ctx.accounts.ibc_app;
    let client_sequence = &mut ctx.accounts.client_sequence;
    let packet_commitment = &mut ctx.accounts.packet_commitment;
    let clock = &ctx.accounts.clock;

    // Check if app_caller is authorized - it must be a PDA derived from the registered program
    // (since program IDs cannot sign transactions in Solana)
    let (expected_pda, _) =
        Pubkey::find_program_address(&[b"router_caller"], &ibc_app.app_program_id);

    require!(
        ctx.accounts.app_caller.key() == expected_pda,
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

    let sequence = client_sequence.next_sequence_send;
    client_sequence.next_sequence_send += 1;

    let counterparty_client_id = ctx.accounts.client.counterparty_info.client_id.clone();

    let packet = Packet {
        sequence,
        source_client: msg.source_client.clone(),
        dest_client: counterparty_client_id,
        timeout_timestamp: msg.timeout_timestamp,
        payloads: vec![msg.payload],
    };

    let commitment = ics24::packet_commitment_bytes32(&packet);
    packet_commitment.value = commitment;

    emit!(SendPacketEvent {
        client_id: msg.source_client,
        sequence,
        packet,
        timeout_timestamp: msg.timeout_timestamp
    });

    Ok(sequence)
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
    use solana_sdk::sysvar::SysvarId;
    use solana_sdk::{clock::Clock, system_program};

    struct SendPacketTestContext {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
        packet_commitment_pubkey: Pubkey,
        client_sequence_pubkey: Pubkey,
        sequence: u64,
    }

    struct SendPacketTestParams {
        client_id: &'static str,
        port_id: &'static str,
        app_program_id: Option<Pubkey>,
        unauthorized_app_caller: Option<Pubkey>,
        active_client: bool,
        current_timestamp: i64,
        timeout_timestamp: i64,
        initial_sequence: u64,
    }

    impl Default for SendPacketTestParams {
        fn default() -> Self {
            Self {
                client_id: "test-client",
                port_id: "test-port",
                app_program_id: None,
                unauthorized_app_caller: None,
                active_client: true,
                current_timestamp: 1000,
                timeout_timestamp: 2000,
                initial_sequence: 0,
            }
        }
    }

    fn setup_send_packet_test_with_params(params: SendPacketTestParams) -> SendPacketTestContext {
        let authority = Pubkey::new_unique();
        let app_program_id = params.app_program_id.unwrap_or_else(Pubkey::new_unique);
        let (default_app_caller, _) =
            Pubkey::find_program_address(&[b"router_caller"], &app_program_id);
        let app_caller = params.unauthorized_app_caller.unwrap_or(default_app_caller);
        let payer = app_caller;

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            params.client_id,
            authority,
            Pubkey::new_unique(),
            "counterparty-client",
            params.active_client,
        );
        let (client_sequence_pda, client_sequence_data) =
            setup_client_sequence(params.client_id, params.initial_sequence);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(params.port_id, app_program_id);

        let clock_data = create_clock_data(params.current_timestamp);

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

        let (packet_commitment_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_COMMITMENT_SEED,
                msg.source_client.as_bytes(),
                &params.initial_sequence.to_le_bytes(),
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
                AccountMeta::new_readonly(app_caller, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda, false),
            ],
            data: crate::instruction::SendPacket { msg }.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_pda, client_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account_with_data(clock_data),
        ];

        SendPacketTestContext {
            instruction,
            accounts,
            packet_commitment_pubkey: packet_commitment_pda,
            client_sequence_pubkey: client_sequence_pda,
            sequence: params.initial_sequence,
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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

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
        assert_eq!(next_sequence, 1); // Should be incremented from 0 to 1
    }

    #[test]
    fn test_send_packet_unauthorized_sender() {
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            unauthorized_app_caller: Some(Pubkey::new_unique()),
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_client_not_active() {
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            active_client: false,
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ClientNotActive as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_send_packet_invalid_timeout() {
        let ctx = setup_send_packet_test_with_params(SendPacketTestParams {
            current_timestamp: 1000,
            timeout_timestamp: 900, // Past timestamp
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let result = mollusk.process_instruction(&ctx.instruction, &ctx.accounts);

        // Check that packet was created with sequence 5
        let commitment_data = get_account_data_from_mollusk(&result, &ctx.packet_commitment_pubkey)
            .expect("packet commitment account not found");
        assert_ne!(commitment_data[..32], [0u8; 32]); // Commitment should be set

        // Check sequence was incremented to 6
        let next_sequence =
            get_client_sequence_from_result_by_pubkey(&result, &ctx.client_sequence_pubkey)
                .expect("client_sequence not found");
        assert_eq!(next_sequence, 6);
    }

    #[test]
    fn test_send_packet_independent_client_sequences() {
        // Test that two different clients have independent sequence counters
        let authority = Pubkey::new_unique();
        let app_program_id = Pubkey::new_unique();
        let (app_caller_pda, _) =
            Pubkey::find_program_address(&[b"router_caller"], &app_program_id);
        let payer = app_program_id;
        let port_id = "test-port";

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program_id);

        // Create first client with sequence 10
        let client_id_1 = "test-client-1";
        let (client_pda_1, client_data_1) = setup_client(
            client_id_1,
            authority,
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
            authority,
            Pubkey::new_unique(),
            "counterparty-client-2",
            true,
        );
        let (client_sequence_pda_2, client_sequence_data_2) =
            setup_client_sequence(client_id_2, 20);

        let clock_data = create_clock_data(1000);

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

        let (packet_commitment_pda_1, _) = Pubkey::find_program_address(
            &[
                PACKET_COMMITMENT_SEED,
                msg_1.source_client.as_bytes(),
                &10u64.to_le_bytes(), // sequence 10
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
                AccountMeta::new_readonly(app_caller_pda, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda_1, false),
            ],
            data: crate::instruction::SendPacket { msg: msg_1 }.data(),
        };

        let accounts_1 = vec![
            create_account(router_state_pda, router_state_data.clone(), crate::ID),
            create_account(ibc_app_pda, ibc_app_data.clone(), crate::ID),
            create_account(client_pda_1, client_data_1, crate::ID),
            create_account(client_sequence_pda_1, client_sequence_data_1, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda_1),
            create_system_account(app_caller_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account_with_data(clock_data.clone()),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());
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

        let (packet_commitment_pda_2, _) = Pubkey::find_program_address(
            &[
                PACKET_COMMITMENT_SEED,
                msg_2.source_client.as_bytes(),
                &20u64.to_le_bytes(), // sequence 20
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
                AccountMeta::new_readonly(app_caller_pda, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda_2, false),
            ],
            data: crate::instruction::SendPacket { msg: msg_2 }.data(),
        };

        let accounts_2 = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_pda_2, client_data_2, crate::ID),
            create_account(client_sequence_pda_2, client_sequence_data_2, crate::ID),
            create_uninitialized_commitment_account(packet_commitment_pda_2),
            create_system_account(app_caller_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account_with_data(clock_data),
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
}
