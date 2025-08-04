use crate::errors::RouterError;
use crate::instructions::light_client_cpi::{verify_membership_cpi, LightClientVerification};
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use solana_light_client_interface::MembershipMsg;

#[derive(Accounts)]
#[instruction(msg: MsgRecvPacket)]
pub struct RecvPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [IBC_APP_SEED, msg.packet.payloads[0].dest_port.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    #[account(
        mut,
        seeds = [CLIENT_SEQUENCE_SEED, msg.packet.dest_client.as_bytes()],
        bump
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE,
        seeds = [
            PACKET_RECEIPT_SEED,
            msg.packet.dest_client.as_bytes(),
            &msg.packet.sequence.to_le_bytes()
        ],
        bump
    )]
    pub packet_receipt: Account<'info, Commitment>,

    #[account(
        init,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE,
        seeds = [
            PACKET_ACK_SEED,
            msg.packet.dest_client.as_bytes(),
            &msg.packet.sequence.to_le_bytes()
        ],
        bump
    )]
    pub packet_ack: Account<'info, Commitment>,

    pub relayer: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,

    pub clock: Sysvar<'info, Clock>,

    // Client for light client lookup
    #[account(
        seeds = [CLIENT_SEED, msg.packet.dest_client.as_bytes()],
        bump,
        constraint = client.active @ RouterError::ClientNotActive,
    )]
    pub client: Account<'info, Client>,

    // Light client verification accounts
    /// CHECK: Light client program, validated against client registry
    pub light_client_program: AccountInfo<'info>,

    /// CHECK: Client state account, owned by light client program
    pub client_state: AccountInfo<'info>,

    /// CHECK: Consensus state account, owned by light client program
    pub consensus_state: AccountInfo<'info>,
}

pub fn recv_packet(ctx: Context<RecvPacket>, msg: MsgRecvPacket) -> Result<()> {
    // TODO: Support multi-payload packets #602
    let router_state = &ctx.accounts.router_state;
    let packet_receipt = &mut ctx.accounts.packet_receipt;
    let packet_ack = &mut ctx.accounts.packet_ack;
    let client = &ctx.accounts.client;
    let clock = &ctx.accounts.clock;

    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    require!(
        msg.packet.payloads.len() == 1,
        RouterError::MultiPayloadPacketNotSupported
    );

    require!(
        msg.packet.source_client == client.counterparty_info.client_id,
        RouterError::InvalidCounterpartyClient
    );

    let current_timestamp = clock.unix_timestamp;
    require!(
        msg.packet.timeout_timestamp > current_timestamp,
        RouterError::InvalidTimeoutTimestamp
    );

    // Verify packet commitment on counterparty chain via light client
    let light_client_verification = LightClientVerification {
        light_client_program: ctx.accounts.light_client_program.clone(),
        client_state: ctx.accounts.client_state.clone(),
        consensus_state: ctx.accounts.consensus_state.clone(),
    };

    let commitment_path =
        ics24::packet_commitment_path(&msg.packet.source_client, msg.packet.sequence);

    let expected_commitment = ics24::packet_commitment_bytes32(&msg.packet);

    // Verify membership proof via CPI to light client
    let membership_msg = MembershipMsg {
        height: msg.proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: msg.proof_commitment.clone(),
        path: vec![commitment_path],
        value: expected_commitment.to_vec(),
    };

    verify_membership_cpi(client, &light_client_verification, membership_msg)?;

    // Check if receipt already exists
    let receipt_commitment = ics24::packet_receipt_commitment_bytes32(&msg.packet);

    // Check if packet was not created by anchor via init_if_needed
    // I.e. it was already saved before
    if packet_receipt.value != [0u8; 32] {
        // Receipt already exists - verify it matches
        if packet_receipt.value == receipt_commitment {
            // No-op: already received with same commitment
            emit!(NoopEvent {});
            return Ok(());
        }

        return Err(RouterError::PacketReceiptMismatch.into());
    }

    packet_receipt.value = receipt_commitment;

    // TODO: CPI to IBC app's onRecvPacket
    // For now, we'll create a simple acknowledgement
    let ack_data = b"packet received".to_vec();

    let acks = vec![ack_data];
    let ack_commitment = ics24::packet_acknowledgement_commitment_bytes32(&acks)?;
    packet_ack.value = ack_commitment;

    emit!(WriteAcknowledgementEvent {
        client_id: msg.packet.dest_client.clone(),
        sequence: msg.packet.sequence,
        packet_data: msg.packet.try_to_vec().unwrap(),
        acknowledgements: acks,
    });

    Ok(())
}

#[event]
pub struct WriteAcknowledgementEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
    pub acknowledgements: Vec<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::sysvar::SysvarId;
    use solana_sdk::{clock::Clock, native_loader, system_program};

    #[test]
    fn test_recv_packet_unauthorized_sender() {
        let authority = Pubkey::new_unique();
        let unauthorized_relayer = Pubkey::new_unique(); // Different from authority
        let payer = unauthorized_relayer;
        let client_id = "test-client";
        let source_client_id = "source-client";
        let port_id = "test-port";
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            light_client_program,
            source_client_id,
            true,
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);

        let packet =
            create_test_packet(1, source_client_id, client_id, "source-port", port_id, 1000);

        let msg = MsgRecvPacket {
            packet,
            proof_commitment: vec![0u8; 32],
            proof_height: 100,
        };

        let (packet_receipt_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_RECEIPT_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let (packet_ack_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_ACK_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::RecvPacket { msg };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_receipt_pda, false),
                AccountMeta::new(packet_ack_pda, false),
                AccountMeta::new_readonly(unauthorized_relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda, false),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(client_state, false),
                AccountMeta::new_readonly(consensus_state, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_account(packet_receipt_pda),
            create_uninitialized_account(packet_ack_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account(),
            create_account(client_pda, client_data, crate::ID),
            create_program_account(light_client_program),
            create_account(client_state, vec![0u8; 100], light_client_program),
            create_account(consensus_state, vec![0u8; 100], light_client_program),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_recv_packet_invalid_counterparty() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let payer = authority;
        let client_id = "test-client";
        let port_id = "test-port";
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        // Client expects counterparty "expected-source-client"
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            light_client_program,
            "expected-source-client",
            true,
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);

        // But packet comes from "wrong-source-client"
        let packet = create_test_packet(
            1,
            "wrong-source-client",
            client_id,
            "source-port",
            port_id,
            1000,
        );

        let msg = MsgRecvPacket {
            packet,
            proof_commitment: vec![0u8; 32],
            proof_height: 100,
        };

        let (packet_receipt_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_RECEIPT_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let (packet_ack_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_ACK_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::RecvPacket { msg };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_receipt_pda, false),
                AccountMeta::new(packet_ack_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda, false),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(client_state, false),
                AccountMeta::new_readonly(consensus_state, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            (
                router_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: router_state_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                ibc_app_pda,
                Account {
                    lamports: 1_000_000,
                    data: ibc_app_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                client_sequence_pda,
                Account {
                    lamports: 1_000_000,
                    data: client_sequence_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                packet_receipt_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                packet_ack_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payer,
                Account {
                    lamports: 10_000_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                system_program::ID,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            (
                client_pda,
                Account {
                    lamports: 1_000_000,
                    data: client_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                Clock::id(),
                Account {
                    lamports: 1,
                    data: vec![1u8; Clock::size_of()],
                    owner: solana_sdk::sysvar::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                light_client_program,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            (
                client_state,
                Account {
                    lamports: 1_000_000,
                    data: vec![0u8; 100],
                    owner: light_client_program,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                consensus_state,
                Account {
                    lamports: 1_000_000,
                    data: vec![0u8; 100],
                    owner: light_client_program,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidCounterpartyClient as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_recv_packet_timeout_expired() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let payer = authority;
        let client_id = "test-client";
        let source_client_id = "source-client";
        let port_id = "test-port";
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            light_client_program,
            source_client_id,
            true,
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);

        // Create clock with current timestamp
        let current_timestamp = 2000;
        let mut clock_data = vec![0u8; Clock::size_of()];
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: current_timestamp,
        };
        bincode::serialize_into(&mut clock_data[..], &clock).unwrap();

        // Packet with expired timeout
        let packet = create_test_packet(
            1,
            source_client_id,
            client_id,
            "source-port",
            port_id,
            current_timestamp - 100, // Expired
        );

        let msg = MsgRecvPacket {
            packet,
            proof_commitment: vec![0u8; 32],
            proof_height: 100,
        };

        let (packet_receipt_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_RECEIPT_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let (packet_ack_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_ACK_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::RecvPacket { msg };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_receipt_pda, false),
                AccountMeta::new(packet_ack_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda, false),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(client_state, false),
                AccountMeta::new_readonly(consensus_state, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_account(packet_receipt_pda),
            create_uninitialized_account(packet_ack_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account_with_data(clock_data),
            create_account(client_pda, client_data, crate::ID),
            create_program_account(light_client_program),
            create_account(client_state, vec![0u8; 100], light_client_program),
            create_account(consensus_state, vec![0u8; 100], light_client_program),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidTimeoutTimestamp as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_recv_packet_client_not_active() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let payer = authority;
        let client_id = "test-client";
        let source_client_id = "source-client";
        let port_id = "test-port";
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        // Create inactive client
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            light_client_program,
            source_client_id,
            false, // Client is not active
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);

        let packet = create_test_packet(
            1,
            source_client_id,
            client_id,
            "source-port",
            port_id,
            1000,
        );

        let msg = MsgRecvPacket {
            packet,
            proof_commitment: vec![0u8; 32],
            proof_height: 100,
        };

        let (packet_receipt_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_RECEIPT_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let (packet_ack_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_ACK_SEED,
                msg.packet.dest_client.as_bytes(),
                &msg.packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::RecvPacket { msg };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new(packet_receipt_pda, false),
                AccountMeta::new(packet_ack_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(Clock::id(), false),
                AccountMeta::new_readonly(client_pda, false),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(client_state, false),
                AccountMeta::new_readonly(consensus_state, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            create_uninitialized_account(packet_receipt_pda),
            create_uninitialized_account(packet_ack_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account(),
            create_account(client_pda, client_data, crate::ID),
            create_program_account(light_client_program),
            create_account(client_state, vec![0u8; 100], light_client_program),
            create_account(consensus_state, vec![0u8; 100], light_client_program),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ClientNotActive as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
