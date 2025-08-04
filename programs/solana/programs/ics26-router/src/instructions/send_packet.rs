use crate::errors::RouterError;
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;

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

    require!(
        ctx.accounts.app_caller.key() == ibc_app.app_program_id,
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
        packet_data: packet.try_to_vec().unwrap(),
    });

    Ok(sequence)
}

#[event]
pub struct SendPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::sysvar::SysvarId;
    use solana_sdk::{clock::Clock, system_program};

    #[test]
    fn test_send_packet_unauthorized_sender() {
        let authority = Pubkey::new_unique();
        let app_program_id = Pubkey::new_unique();
        let unauthorized_app_caller = Pubkey::new_unique(); // Different from app_program_id
        let payer = unauthorized_app_caller;
        let client_id = "test-client";
        let port_id = "test-port";

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            Pubkey::new_unique(),
            "counterparty-client",
            true,
        );
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program_id);

        let msg = MsgSendPacket {
            source_client: client_id.to_string(),
            timeout_timestamp: 1000,
            payload: Payload {
                source_port: port_id.to_string(),
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
                &0u64.to_le_bytes(), // Uses current sequence
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::SendPacket { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_commitment_pda, false),
                AccountMeta::new_readonly(unauthorized_app_caller, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_pda, client_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_account(packet_commitment_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account(),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_send_packet_client_not_active() {
        let authority = Pubkey::new_unique();
        let app_program_id = Pubkey::new_unique();
        let app_caller = app_program_id;
        let payer = app_caller;
        let client_id = "test-client";
        let port_id = "test-port";

        let (router_state_pda, router_state_data) = setup_router_state(authority);

        // Create inactive client
        let (client_pda, _) =
            Pubkey::find_program_address(&[CLIENT_SEED, client_id.as_bytes()], &crate::ID);
        let inactive_client = Client {
            client_id: client_id.to_string(),
            client_program_id: Pubkey::new_unique(),
            counterparty_info: CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                connection_id: "connection-0".to_string(),
                merkle_prefix: vec![0x01, 0x02, 0x03],
            },
            authority,
            active: false, // Client is not active
        };
        let client_data = create_account_data(&inactive_client);

        let (client_sequence_pda, _) =
            Pubkey::find_program_address(&[CLIENT_SEQUENCE_SEED, client_id.as_bytes()], &crate::ID);
        let client_sequence = ClientSequence {
            next_sequence_send: 0,
        };
        let client_sequence_data = create_account_data(&client_sequence);

        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program_id);

        let msg = MsgSendPacket {
            source_client: client_id.to_string(),
            timeout_timestamp: 1000,
            payload: Payload {
                source_port: port_id.to_string(),
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
                &0u64.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::SendPacket { msg };

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
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_pda, client_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_account(packet_commitment_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account(),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ClientNotActive as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_send_packet_invalid_timeout() {
        let authority = Pubkey::new_unique();
        let app_program_id = Pubkey::new_unique();
        let app_caller = app_program_id;
        let payer = app_caller;
        let client_id = "test-client";
        let port_id = "test-port";

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            Pubkey::new_unique(),
            "counterparty-client",
            true,
        );
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program_id);

        // Create clock with current timestamp
        let current_timestamp = 1000;
        let mut clock_data = vec![0u8; Clock::size_of()];
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: current_timestamp,
        };
        bincode::serialize_into(&mut clock_data[..], &clock).unwrap();

        // Invalid: timeout is in the past
        let msg = MsgSendPacket {
            source_client: client_id.to_string(),
            timeout_timestamp: current_timestamp - 100, // Past timestamp
            payload: Payload {
                source_port: port_id.to_string(),
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
                &0u64.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::SendPacket { msg };

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
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_pda, client_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_account(packet_commitment_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account_with_data(clock_data),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidTimeoutTimestamp as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_send_packet_timeout_duration_too_long() {
        let authority = Pubkey::new_unique();
        let app_program_id = Pubkey::new_unique();
        let app_caller = app_program_id;
        let payer = app_caller;
        let client_id = "test-client";
        let port_id = "test-port";

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            Pubkey::new_unique(),
            "counterparty-client",
            true,
        );
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program_id);

        // Create clock with current timestamp
        let current_timestamp = 1000;
        let mut clock_data = vec![0u8; Clock::size_of()];
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: current_timestamp,
        };
        bincode::serialize_into(&mut clock_data[..], &clock).unwrap();

        // Invalid: timeout duration exceeds MAX_TIMEOUT_DURATION
        let msg = MsgSendPacket {
            source_client: client_id.to_string(),
            timeout_timestamp: current_timestamp + MAX_TIMEOUT_DURATION + 1, // Too far in future
            payload: Payload {
                source_port: port_id.to_string(),
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
                &0u64.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::SendPacket { msg };

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
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_pda, client_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_account(packet_commitment_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account_with_data(clock_data),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidTimeoutDuration as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
