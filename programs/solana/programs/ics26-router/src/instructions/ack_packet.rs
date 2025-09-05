use crate::errors::RouterError;
use crate::router_cpi::on_acknowledgement_packet_cpi;
use crate::router_cpi::{verify_membership_cpi, LightClientVerification};
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use ics25_handler::MembershipMsg;
#[cfg(test)]
use solana_ibc_types::router::APP_STATE_SEED;

#[derive(Accounts)]
#[instruction(msg: MsgAckPacket)]
pub struct AckPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [IBC_APP_SEED, msg.packet.payloads[0].source_port.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    #[account(
        mut,
        seeds = [
            PACKET_COMMITMENT_SEED,
            msg.packet.source_client.as_bytes(),
            &msg.packet.sequence.to_le_bytes()
        ],
        bump
    )]
    /// CHECK: We manually verify this account and handle the case where it doesn't exist
    pub packet_commitment: AccountInfo<'info>,

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

    // Client for light client lookup
    #[account(
        seeds = [CLIENT_SEED, msg.packet.source_client.as_bytes()],
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

pub fn ack_packet(ctx: Context<AckPacket>, msg: MsgAckPacket) -> Result<()> {
    // TODO: Support multi-payload packets #602
    let router_state = &ctx.accounts.router_state;
    let packet_commitment_account = &ctx.accounts.packet_commitment;
    let client = &ctx.accounts.client;

    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    require!(
        msg.packet.payloads.len() == 1,
        RouterError::MultiPayloadPacketNotSupported
    );

    require!(
        msg.packet.dest_client == client.counterparty_info.client_id,
        RouterError::InvalidCounterpartyClient
    );

    // Verify acknowledgement proof on counterparty chain via light client
    let light_client_verification = LightClientVerification {
        light_client_program: ctx.accounts.light_client_program.clone(),
        client_state: ctx.accounts.client_state.clone(),
        consensus_state: ctx.accounts.consensus_state.clone(),
    };

    let ack_path =
        ics24::packet_acknowledgement_commitment_path(&msg.packet.dest_client, msg.packet.sequence);

    let membership_msg = MembershipMsg {
        height: msg.proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: msg.proof_acked.clone(),
        path: vec![ack_path],
        value: msg.acknowledgement.clone(),
    };

    verify_membership_cpi(client, &light_client_verification, membership_msg)?;

    // Check if packet commitment exists (no-op case)
    // An uninitialized account will be owned by System Program
    if packet_commitment_account.owner != &crate::ID || packet_commitment_account.data_is_empty() {
        emit!(NoopEvent {});
        return Ok(());
    }

    // Safe to deserialize since we know it's owned by our program
    // Verify the commitment value
    {
        let data = packet_commitment_account.try_borrow_data()?;
        let packet_commitment = Commitment::try_from_slice(&data[8..])?;
        let expected_commitment = ics24::packet_commitment_bytes32(&msg.packet);
        require!(
            packet_commitment.value == expected_commitment,
            RouterError::PacketCommitmentMismatch
        );
    }

    on_acknowledgement_packet_cpi(
        &ctx.accounts.ibc_app_program,
        &ctx.accounts.ibc_app_state,
        &ctx.accounts.router_program,
        &ctx.accounts.payer,
        &ctx.accounts.system_program,
        &msg.packet,
        &msg.packet.payloads[0],
        &msg.acknowledgement,
        &ctx.accounts.relayer.key(),
    )?;

    // Close the account and return rent to payer
    let dest_starting_lamports = ctx.accounts.payer.lamports();
    **ctx.accounts.payer.lamports.borrow_mut() = dest_starting_lamports
        .checked_add(packet_commitment_account.lamports())
        .ok_or(RouterError::ArithmeticOverflow)?;
    **packet_commitment_account.lamports.borrow_mut() = 0;

    let mut data = packet_commitment_account.try_borrow_mut_data()?;
    data.fill(0);

    emit!(AckPacketEvent {
        client_id: msg.packet.source_client.clone(),
        sequence: msg.packet.sequence,
        packet_data: msg.packet.try_to_vec()?,
        acknowledgement: msg.acknowledgement,
    });

    Ok(())
}

#[event]
pub struct AckPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
    pub acknowledgement: Vec<u8>,
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
    use solana_sdk::system_program;

    struct AckPacketTestContext {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
        packet_commitment_pubkey: Pubkey,
        packet: Packet,
        dummy_app_state_pubkey: Pubkey,
    }

    struct AckPacketTestParams {
        source_client_id: &'static str,
        dest_client_id: &'static str,
        port_id: &'static str,
        app_program_id: Option<Pubkey>,
        unauthorized_relayer: Option<Pubkey>,
        wrong_dest_client: Option<&'static str>,
        active_client: bool,
        initial_sequence: u64,
        acknowledgement: Vec<u8>,
        proof_height: u64,
        with_existing_commitment: bool,
    }

    impl Default for AckPacketTestParams {
        fn default() -> Self {
            Self {
                source_client_id: "source-client",
                dest_client_id: "dest-client",
                port_id: "test-port",
                app_program_id: None,
                unauthorized_relayer: None,
                wrong_dest_client: None,
                active_client: true,
                initial_sequence: 1,
                acknowledgement: vec![1, 2, 3, 4],
                proof_height: 100,
                with_existing_commitment: true,
            }
        }
    }

    fn setup_ack_packet_test_with_params(params: AckPacketTestParams) -> AckPacketTestContext {
        let authority = Pubkey::new_unique();
        let relayer = params.unauthorized_relayer.unwrap_or(authority);
        let payer = relayer;
        let app_program_id = params.app_program_id.unwrap_or(MOCK_IBC_APP_PROGRAM_ID);
        let light_client_program = MOCK_LIGHT_CLIENT_ID;

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            params.source_client_id,
            authority,
            light_client_program,
            params.dest_client_id,
            params.active_client,
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(params.port_id, app_program_id);

        // Mock app state - just create a dummy account since mock app doesn't use it
        let (dummy_app_state_pda, _) =
            Pubkey::find_program_address(&[APP_STATE_SEED], &app_program_id);

        let packet_dest_client = params.wrong_dest_client.unwrap_or(params.dest_client_id);
        let packet = create_test_packet(
            params.initial_sequence,
            params.source_client_id,
            packet_dest_client,
            params.port_id,
            "dest-port",
            1000,
        );

        let (packet_commitment_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_COMMITMENT_SEED,
                packet.source_client.as_bytes(),
                &packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let msg = MsgAckPacket {
            packet: packet.clone(),
            acknowledgement: params.acknowledgement,
            proof_acked: vec![0u8; 32],
            proof_height: params.proof_height,
        };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(packet_commitment_pda, false),
                AccountMeta::new_readonly(app_program_id, false),
                AccountMeta::new(dummy_app_state_pda, false),
                AccountMeta::new_readonly(crate::ID, false), // router_program
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(client_pda, false),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(client_state, false),
                AccountMeta::new_readonly(consensus_state, false),
            ],
            data: crate::instruction::AckPacket { msg }.data(),
        };

        let packet_commitment_account = if params.with_existing_commitment {
            let (_, data) =
                setup_packet_commitment(params.source_client_id, packet.sequence, &packet);
            create_account(packet_commitment_pda, data, crate::ID)
        } else {
            create_uninitialized_account(packet_commitment_pda, 0)
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            packet_commitment_account,
            create_bpf_program_account(app_program_id),
            create_account(dummy_app_state_pda, vec![0u8; 32], app_program_id), // Mock app state
            create_bpf_program_account(crate::ID),                              // router_program
            create_system_account(relayer), // relayer (also signer)
            create_system_account(payer),   // payer (also signer)
            create_program_account(system_program::ID),
            create_account(client_pda, client_data, crate::ID),
            create_bpf_program_account(light_client_program),
            create_account(client_state, vec![0u8; 100], light_client_program),
            create_account(consensus_state, vec![0u8; 100], light_client_program),
        ];

        AckPacketTestContext {
            instruction,
            accounts,
            packet_commitment_pubkey: packet_commitment_pda,
            packet,
            dummy_app_state_pubkey: dummy_app_state_pda,
        }
    }

    #[test]
    fn test_ack_packet_success() {
        let ctx = setup_ack_packet_test_with_params(AckPacketTestParams::default());

        let mollusk = setup_mollusk_with_mock_programs();

        let payer_pubkey = ctx.accounts[7].0; // Payer is at index 7
        let initial_payer_lamports = ctx.accounts[7].1.lamports;
        let commitment_lamports = ctx.accounts[2].1.lamports; // Packet commitment is at index 2

        let checks = vec![
            Check::success(),
            // Verify packet commitment account is closed (0 lamports)
            Check::account(&ctx.packet_commitment_pubkey)
                .lamports(0)
                .build(),
            // Verify payer received the rent back
            Check::account(&payer_pubkey)
                .lamports(initial_payer_lamports + commitment_lamports)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);

        // Mock app doesn't track counters, so we just verify the instruction succeeded
    }

    #[test]
    fn test_ack_packet_noop_no_commitment() {
        let ctx = setup_ack_packet_test_with_params(AckPacketTestParams {
            with_existing_commitment: false, // No packet commitment exists
            ..Default::default()
        });

        let mollusk = setup_mollusk_with_mock_programs();

        // When packet commitment doesn't exist, it should succeed (noop)
        let checks = vec![Check::success()];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_ack_packet_unauthorized_sender() {
        let ctx = setup_ack_packet_test_with_params(AckPacketTestParams {
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
    fn test_ack_packet_invalid_counterparty() {
        let ctx = setup_ack_packet_test_with_params(AckPacketTestParams {
            wrong_dest_client: Some("wrong-dest-client"),
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidCounterpartyClient as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_ack_packet_client_not_active() {
        let ctx = setup_ack_packet_test_with_params(AckPacketTestParams {
            active_client: false,
            ..Default::default()
        });

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::ClientNotActive as u32,
        ))];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }
}
