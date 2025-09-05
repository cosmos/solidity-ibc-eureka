use crate::errors::RouterError;
use crate::router_cpi::on_recv_packet_cpi;
use crate::router_cpi::{verify_membership_cpi, LightClientVerification};
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use ics25_handler::MembershipMsg;

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

    // IBC app accounts for CPI
    /// CHECK: IBC app program, validated against `IBCApp` account
    #[account(
        constraint = ibc_app_program.key() == ibc_app.app_program_id @ RouterError::IbcAppNotFound
    )]
    pub ibc_app_program: AccountInfo<'info>,

    /// CHECK: IBC app state account, owned by IBC app program
    pub ibc_app_state: AccountInfo<'info>,

    /// The router program account (this program)
    /// CHECK: This will be verified in the CPI as the calling program
    #[account(address = crate::ID)]
    pub router_program: AccountInfo<'info>,

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

    let acknowledgement = match on_recv_packet_cpi(
        &ctx.accounts.ibc_app_program,
        &ctx.accounts.ibc_app_state,
        &ctx.accounts.router_program,
        &ctx.accounts.payer,
        &ctx.accounts.system_program,
        &msg.packet,
        &msg.packet.payloads[0],
        &ctx.accounts.relayer.key(),
    ) {
        Ok(ack) => {
            require!(
                !ack.is_empty(),
                RouterError::AsyncAcknowledgementNotSupported
            );

            // If the app returns the universal error acknowledgement, we accept it
            // (don't revert, just use it as the acknowledgement)
            ack
        }
        Err(e) => {
            // If the CPI fails, use universal error ack
            // In Solana, we can't easily check if it's OOG vs other errors,
            // but we do check that we got an error (not empty)
            require!(!e.to_string().is_empty(), RouterError::FailedCallback);

            msg!("IBC app recv packet callback error: {:?}", e);

            ics24::UNIVERSAL_ERROR_ACK.to_vec()
        }
    };

    let acks = vec![acknowledgement];
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

        let mollusk = setup_mollusk_with_mock_programs();

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
    }

    impl Default for RecvPacketTestParams {
        fn default() -> Self {
            Self {
                active_client: true,
                timeout_offset: 1000,
                source_client_id: "source-client",
                unauthorized_relayer: None,
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
        let ibc_app_program_id = MOCK_IBC_APP_PROGRAM_ID;
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, ibc_app_program_id);
        let ibc_app_state = Pubkey::new_unique();
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
                AccountMeta::new_readonly(ibc_app_program_id, false),
                AccountMeta::new(ibc_app_state, false),
                AccountMeta::new_readonly(crate::ID, false), // router_program
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

        let packet_receipt_account = create_uninitialized_commitment_account(packet_receipt_pda);
        let packet_ack_account = create_uninitialized_commitment_account(packet_ack_pda);

        // Create signer account (relayer and payer are the same)
        let signer_account = create_system_account(relayer);

        // Accounts must be in the exact order of the instruction
        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_account(client_sequence_pda, client_sequence_data, crate::ID),
            packet_receipt_account,
            packet_ack_account,
            create_bpf_program_account(ibc_app_program_id),
            create_account(ibc_app_state, vec![0u8; 100], ibc_app_program_id),
            create_bpf_program_account(crate::ID), // router_program
            signer_account.clone(),                // relayer
            signer_account,                        // payer (same account as relayer)
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

    #[test]
    fn test_recv_packet_success() {
        let ctx = setup_recv_packet_test(true, 1000);

        let mollusk = setup_mollusk_with_mock_programs();

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

        // Check acknowledgement - mock app returns "packet received"
        // Just verify that an acknowledgement was written (non-zero)
        let ack_data = get_account_data_from_mollusk(&result, &ctx.packet_ack_pubkey)
            .expect("packet ack account not found");
        // Verify the acknowledgement commitment is not empty (all zeros)
        assert_ne!(ack_data[..32], [0u8; 32], "acknowledgement should be set");
    }

    #[test]
    fn test_recv_packet_app_returns_universal_error_ack() {
        // Create packet with special data that triggers error ack from mock app
        let mut ctx = setup_recv_packet_test(true, 1000);

        // Modify packet data to trigger error acknowledgement
        let packet = &mut ctx.packet;
        packet.payloads[0].value = b"RETURN_ERROR_ACK_test_data".to_vec();

        // Update the instruction with modified packet
        let msg = MsgRecvPacket {
            packet: packet.clone(),
            proof_commitment: vec![0u8; 32],
            proof_height: 100,
        };

        ctx.instruction.data = crate::instruction::RecvPacket { msg }.data();

        let mollusk = setup_mollusk_with_mock_programs();

        // Calculate expected rent-exempt lamports for Commitment accounts
        let commitment_rent = {
            use anchor_lang::Space;
            use solana_sdk::rent::Rent;
            let account_size = 8 + Commitment::INIT_SPACE;
            Rent::default().minimum_balance(account_size)
        };

        let checks = vec![
            Check::success(), // Should still succeed even with error ack
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

        // Check acknowledgement contains universal error ack
        let ack_data = get_account_data_from_mollusk(&result, &ctx.packet_ack_pubkey)
            .expect("packet ack account not found");

        // The ack commitment should be the keccak256 of the acks vector containing b"error"
        let expected_acks = vec![b"error".to_vec()];
        let expected_ack_commitment =
            ics24::packet_acknowledgement_commitment_bytes32(&expected_acks)
                .expect("failed to compute ack commitment");

        assert_eq!(
            ack_data[..32],
            expected_ack_commitment,
            "acknowledgement should be universal error ack"
        );
    }

    // Note: Testing CPI failures in mollusk is challenging because the test environment
    // propagates CPI errors differently than real Solana runtime. In production,
    // the router would catch CPI failures and use universal error acknowledgement.
    // This behavior is covered by the implementation but not easily testable in mollusk.
}
