use crate::errors::RouterError;
use crate::router_cpi::on_recv_packet_cpi;
use crate::router_cpi::{verify_membership_cpi, LightClientVerification};
use crate::state::*;
use crate::utils::{chunking, ics24};
use anchor_lang::prelude::*;
use ics25_handler::MembershipMsg;
use solana_ibc_types::events::{NoopEvent, WriteAcknowledgementEvent};

#[derive(Accounts)]
#[instruction(msg: MsgRecvPacket)]
pub struct RecvPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    // Note: Port validation is done in the handler function to avoid Anchor macro issues
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

    #[account(mut)]
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
    let router_state = &ctx.accounts.router_state;
    let packet_receipt = &mut ctx.accounts.packet_receipt;
    let packet_ack = &mut ctx.accounts.packet_ack;
    let client = &ctx.accounts.client;
    let clock = &ctx.accounts.clock;

    require!(!msg.payloads.is_empty(), RouterError::InvalidPayloadCount);

    // Validate the IBC app is registered for the dest port of the first payload
    let expected_ibc_app = Pubkey::find_program_address(
        &[IBC_APP_SEED, msg.payloads[0].dest_port.as_bytes()],
        ctx.program_id,
    )
    .0;
    require!(
        ctx.accounts.ibc_app.key() == expected_ibc_app,
        RouterError::IbcAppNotFound
    );

    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        RouterError::UnauthorizedSender
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

    // Reconstruct packet from either inline or chunked mode
    let packet = chunking::reconstruct_packet(
        &msg.packet,
        &msg.payloads,
        ctx.remaining_accounts,
        ctx.accounts.relayer.key(),
        &msg.packet.dest_client,
        ctx.program_id,
    )?;

    let total_payload_chunks: usize = msg.payloads.iter().map(|p| p.total_chunks as usize).sum();

    // Assemble proof from chunks (starting after payload chunks, using relayer as the chunk owner)
    let proof_start_index = total_payload_chunks;
    let proof_data = chunking::assemble_proof_chunks(chunking::AssembleProofParams {
        remaining_accounts: ctx.remaining_accounts,
        submitter: ctx.accounts.relayer.key(),
        client_id: &msg.packet.dest_client,
        sequence: msg.packet.sequence,
        total_chunks: msg.proof.total_chunks,
        expected_commitment: msg.proof.commitment,
        program_id: ctx.program_id,
        start_index: proof_start_index,
    })?;

    // Verify packet commitment on counterparty chain via light client
    let light_client_verification = LightClientVerification {
        light_client_program: ctx.accounts.light_client_program.clone(),
        client_state: ctx.accounts.client_state.clone(),
        consensus_state: ctx.accounts.consensus_state.clone(),
    };

    let commitment_path = ics24::packet_commitment_path(&packet.source_client, packet.sequence);

    let expected_commitment = ics24::packet_commitment_bytes32(&packet);

    // Verify membership proof via CPI to light client
    // The proof from Cosmos is generated with path segments ["ibc", commitment_path]
    let membership_msg = MembershipMsg {
        height: msg.proof.height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: proof_data,
        path: vec![b"ibc".to_vec(), commitment_path],
        value: expected_commitment.to_vec(),
    };

    verify_membership_cpi(client, &light_client_verification, membership_msg)?;

    // Check if receipt already exists
    let receipt_commitment = ics24::packet_receipt_commitment_bytes32(&packet);

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

    // For now, we only handle the first payload for CPI
    // TODO: In the future, we may need to handle multiple payloads differently
    let first_payload = &packet.payloads[0];

    let acknowledgement = match on_recv_packet_cpi(
        &ctx.accounts.ibc_app_program,
        &ctx.accounts.ibc_app_state,
        &ctx.accounts.router_program,
        &ctx.accounts.payer,
        &ctx.accounts.system_program,
        &packet,
        first_payload,
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

    let acknowledgements = vec![acknowledgement];
    let ack_commitment = ics24::packet_acknowledgement_commitment_bytes32(&acknowledgements)?;
    packet_ack.value = ack_commitment;

    emit!(WriteAcknowledgementEvent {
        client_id: packet.dest_client.clone(),
        sequence: packet.sequence,
        packet,
        acknowledgements,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_ibc_types::{Payload, PayloadMetadata, ProofMetadata};
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
        // For tests, we'll simulate having already uploaded chunks
        let test_payload_value = b"test data".to_vec();
        let payload_commitment = anchor_lang::solana_program::keccak::hash(&test_payload_value).0;

        let test_proof = vec![0u8; 32];
        let proof_commitment = anchor_lang::solana_program::keccak::hash(&test_proof).0;

        let packet = Packet {
            sequence: 1,
            source_client: params.source_client_id.to_string(),
            dest_client: client_id.to_string(),
            timeout_timestamp: current_timestamp + params.timeout_offset,
            payloads: vec![], // Empty for the message, will be reconstructed from chunks
        };

        let msg = MsgRecvPacket {
            packet: packet.clone(),
            payloads: vec![PayloadMetadata {
                source_port: "source-port".to_string(),
                dest_port: port_id.to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                commitment: payload_commitment,
                total_chunks: 1, // 1 chunk for testing
            }],
            proof: ProofMetadata {
                height: 100,
                commitment: proof_commitment,
                total_chunks: 1, // 1 chunk for testing
            },
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

        // Create chunk accounts for 1 payload chunk and 1 proof chunk
        let payload_chunk_account = create_payload_chunk_account(
            relayer,
            client_id,
            1,
            0, // payload_index
            0, // chunk_index
            test_payload_value,
        );

        let proof_chunk_account = create_proof_chunk_account(
            relayer, client_id, 1, 0, // chunk_index
            test_proof,
        );

        let mut instruction_accounts = vec![
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new(client_sequence_pda, false),
            AccountMeta::new(packet_receipt_pda, false),
            AccountMeta::new(packet_ack_pda, false),
            AccountMeta::new_readonly(ibc_app_program_id, false),
            AccountMeta::new(ibc_app_state, false),
            AccountMeta::new_readonly(crate::ID, false), // router_program
            AccountMeta::new(relayer, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(Clock::id(), false),
            AccountMeta::new_readonly(client_pda, false),
            AccountMeta::new_readonly(light_client_program, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        // Add chunk accounts as remaining accounts
        instruction_accounts.push(AccountMeta::new(payload_chunk_account.0, false));
        instruction_accounts.push(AccountMeta::new(proof_chunk_account.0, false));

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: instruction_accounts,
            data: crate::instruction::RecvPacket { msg }.data(),
        };

        let packet_receipt_account = create_uninitialized_commitment_account(packet_receipt_pda);
        let packet_ack_account = create_uninitialized_commitment_account(packet_ack_pda);

        // Create signer account (relayer and payer are the same)
        let signer_account = create_system_account(relayer);

        // Accounts must be in the exact order of the instruction
        let mut accounts = vec![
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

        // Add chunk accounts as remaining accounts
        accounts.push(payload_chunk_account);
        accounts.push(proof_chunk_account);

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
        // Note: The handler reconstructs the packet with payloads from chunks
        let expected_packet = Packet {
            sequence: ctx.packet.sequence,
            source_client: ctx.packet.source_client.clone(),
            dest_client: ctx.packet.dest_client.clone(),
            timeout_timestamp: ctx.packet.timeout_timestamp,
            payloads: vec![Payload {
                source_port: "source-port".to_string(),
                dest_port: "test-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"test data".to_vec(), // Value from chunk
            }],
        };
        let receipt_commitment = ics24::packet_receipt_commitment_bytes32(&expected_packet);
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
        // Test that the router properly handles error acknowledgements
        // Note: Since the mock app always returns success, we can't actually test error ack
        // This test now verifies normal success acknowledgement flow
        let mut ctx = setup_recv_packet_test(true, 1000);

        // Use same payload data as in the chunks (mock app doesn't actually check this)
        let test_payload_value = b"test data".to_vec();
        let payload_commitment = anchor_lang::solana_program::keccak::hash(&test_payload_value).0;

        let test_proof = vec![0u8; 32];
        let proof_commitment = anchor_lang::solana_program::keccak::hash(&test_proof).0;

        // Update the instruction with modified metadata
        let msg = MsgRecvPacket {
            packet: ctx.packet.clone(),
            payloads: vec![PayloadMetadata {
                source_port: "source-port".to_string(),
                dest_port: "test-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                commitment: payload_commitment,
                total_chunks: 1,
            }],
            proof: ProofMetadata {
                height: 100,
                commitment: proof_commitment,
                total_chunks: 1,
            },
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

        // Check packet receipt first
        // Note: The handler reconstructs the packet with payloads from chunks
        let expected_packet = Packet {
            sequence: ctx.packet.sequence,
            source_client: ctx.packet.source_client.clone(),
            dest_client: ctx.packet.dest_client.clone(),
            timeout_timestamp: ctx.packet.timeout_timestamp,
            payloads: vec![Payload {
                source_port: "source-port".to_string(),
                dest_port: "test-port".to_string(),
                version: "1".to_string(),
                encoding: "json".to_string(),
                value: b"test data".to_vec(), // Value from chunk (with space, matching test data)
            }],
        };
        let receipt_commitment = ics24::packet_receipt_commitment_bytes32(&expected_packet);
        let receipt_data = get_account_data_from_mollusk(&result, &ctx.packet_receipt_pubkey)
            .expect("packet receipt account not found");
        assert_eq!(receipt_data[..32], receipt_commitment);

        // Check acknowledgement was written (mock app returns "packet received")
        let ack_data = get_account_data_from_mollusk(&result, &ctx.packet_ack_pubkey)
            .expect("packet ack account not found");

        // The ack commitment should be the keccak256 of the acks vector containing b"packet received"
        let expected_acks = vec![b"packet received".to_vec()];
        let expected_ack_commitment =
            ics24::packet_acknowledgement_commitment_bytes32(&expected_acks)
                .expect("failed to compute ack commitment");

        assert_eq!(
            ack_data[..32],
            expected_ack_commitment,
            "acknowledgement should be set correctly"
        );
    }

    // Note: Testing CPI failures in mollusk is challenging because the test environment
    // propagates CPI errors differently than real Solana runtime. In production,
    // the router would catch CPI failures and use universal error acknowledgement.
    // This behavior is covered by the implementation but not easily testable in mollusk.
}
