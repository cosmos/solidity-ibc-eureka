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
        packet_data: msg.packet.try_to_vec()?,
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
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::sysvar::SysvarId;
    use solana_sdk::{clock::Clock, system_program};

    // Mock light client program ID - must match the ID in mock-light-client/src/lib.rs
    const MOCK_LIGHT_CLIENT_ID: Pubkey =
        solana_sdk::pubkey!("4nFbkWTbUxKwXqKHzLdxkUfYZ9MrVkzJp7nXt8GY7JKp");

    #[test]
    fn test_recv_packet_unauthorized_sender() {
        let ctx = setup_recv_packet_test_with_params(RecvPacketTestParams {
            unauthorized_relayer: Some(Pubkey::new_unique()),
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_recv_packet_invalid_counterparty() {
        // Setup expects "source-client" but packet comes from "wrong-source-client"
        let ctx = setup_recv_packet_test_with_params(RecvPacketTestParams {
            source_client_id: "wrong-source-client",
            ..Default::default()
        });

        let mut mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());
        mollusk.add_program(
            &MOCK_LIGHT_CLIENT_ID,
            crate::get_mock_client_program_path(),
            &solana_sdk::bpf_loader_upgradeable::ID,
        );

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidCounterpartyClient as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_recv_packet_timeout_expired() {
        let ctx = setup_recv_packet_test(true, -100); // Expired timeout

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidTimeoutTimestamp as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_recv_packet_client_not_active() {
        let ctx = setup_recv_packet_test(false, 1000); // Inactive client

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ClientNotActive as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    struct RecvPacketTestContext {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
        packet: Packet,
        packet_receipt_pubkey: Pubkey,
        packet_ack_pubkey: Pubkey,
    }

    struct RecvPacketTestParams {
        active_client: bool,
        timeout_offset: i64,
        source_client_id: &'static str,
        unauthorized_relayer: Option<Pubkey>,
        existing_receipt: bool,
    }

    impl Default for RecvPacketTestParams {
        fn default() -> Self {
            Self {
                active_client: true,
                timeout_offset: 1000,
                source_client_id: "source-client",
                unauthorized_relayer: None,
                existing_receipt: false,
            }
        }
    }

    fn setup_recv_packet_test_with_params(params: RecvPacketTestParams) -> RecvPacketTestContext {
        let authority = Pubkey::new_unique();
        let relayer = params.unauthorized_relayer.unwrap_or(authority);
        let payer = relayer;
        let client_id = "test-client";
        let port_id = "test-port";
        let light_client_program = MOCK_LIGHT_CLIENT_ID;

        let (router_state_pda, router_state_data) = setup_router_state(authority);

        // Always setup client expecting "source-client" as counterparty
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            light_client_program,
            "source-client",
            params.active_client,
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());
        let (client_sequence_pda, client_sequence_data) = setup_client_sequence(client_id, 0);

        let current_timestamp = 1000;
        let clock_data = create_clock_data(current_timestamp);

        // Packet uses the source_client_id from params (could be different)
        let packet = create_test_packet(
            1,
            params.source_client_id,
            client_id,
            "source-port",
            port_id,
            current_timestamp + params.timeout_offset,
        );

        let msg = MsgRecvPacket {
            packet: packet.clone(),
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
            data: crate::instruction::RecvPacket { msg }.data(),
        };

        let packet_receipt_account = if params.existing_receipt {
            let receipt_commitment = ics24::packet_receipt_commitment_bytes32(&packet);
            let existing_receipt = Commitment {
                value: receipt_commitment,
            };
            create_account(
                packet_receipt_pda,
                create_account_data(&existing_receipt),
                crate::ID,
            )
        } else {
            create_uninitialized_commitment_account(packet_receipt_pda)
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            packet_receipt_account,
            create_uninitialized_commitment_account(packet_ack_pda),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_clock_account_with_data(clock_data),
            create_account(client_pda, client_data, crate::ID),
            create_bpf_program_account(light_client_program),
            create_account(client_state, vec![0u8; 100], light_client_program),
            create_account(consensus_state, vec![0u8; 100], light_client_program),
        ];

        RecvPacketTestContext {
            instruction,
            accounts,
            packet,
            packet_receipt_pubkey: packet_receipt_pda,
            packet_ack_pubkey: packet_ack_pda,
        }
    }

    fn setup_recv_packet_test(active_client: bool, timeout_offset: i64) -> RecvPacketTestContext {
        setup_recv_packet_test_with_params(RecvPacketTestParams {
            active_client,
            timeout_offset,
            ..Default::default()
        })
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

    fn create_bpf_program_account(pubkey: Pubkey) -> (Pubkey, solana_sdk::account::Account) {
        (
            pubkey,
            solana_sdk::account::Account {
                lamports: 0,
                data: vec![],
                owner: solana_sdk::bpf_loader_upgradeable::ID,
                executable: true,
                rent_epoch: 0,
            },
        )
    }

    #[test]
    fn test_recv_packet_success() {
        let ctx = setup_recv_packet_test(true, 1000);

        let mut mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());
        mollusk.add_program(
            &MOCK_LIGHT_CLIENT_ID,
            crate::get_mock_client_program_path(),
            &solana_sdk::bpf_loader_upgradeable::ID,
        );

        // Calculate expected rent-exempt lamports for Commitment accounts
        let commitment_rent = {
            use anchor_lang::Space;
            use solana_sdk::rent::Rent;
            let account_size = 8 + Commitment::INIT_SPACE;
            Rent::default().minimum_balance(account_size)
        };

        let checks = vec![
            Check::success(),
            Check::account(&ctx.packet_receipt_pubkey)
                .lamports(commitment_rent)
                .owner(&crate::ID)
                .build(),
            Check::account(&ctx.packet_ack_pubkey)
                .lamports(commitment_rent)
                .owner(&crate::ID)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);

        let result = mollusk.process_instruction(&ctx.instruction, &ctx.accounts);

        // Check packet receipt
        let receipt_commitment = ics24::packet_receipt_commitment_bytes32(&ctx.packet);
        let receipt_data = get_account_data_from_mollusk(&result, &ctx.packet_receipt_pubkey)
            .expect("packet receipt account not found");
        assert_eq!(receipt_data[..32], receipt_commitment);

        // Check acknowledgement
        let ack_data = b"packet received".to_vec();
        let expected_ack_commitment =
            ics24::packet_acknowledgement_commitment_bytes32(&[ack_data]).unwrap();
        let ack_data = get_account_data_from_mollusk(&result, &ctx.packet_ack_pubkey)
            .expect("packet ack account not found");
        assert_eq!(ack_data[..32], expected_ack_commitment);
    }

    #[test]
    #[ignore]
    fn test_recv_packet_duplicate_noop() {
        let ctx = setup_recv_packet_test_with_params(RecvPacketTestParams {
            existing_receipt: true,
            ..Default::default()
        });

        let mut mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());
        mollusk.add_program(
            &MOCK_LIGHT_CLIENT_ID,
            crate::get_mock_client_program_path(),
            &solana_sdk::bpf_loader_upgradeable::ID,
        );

        // This is expected to succeed with a no-op since the packet was already received
        let checks = vec![Check::success()];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }
}
