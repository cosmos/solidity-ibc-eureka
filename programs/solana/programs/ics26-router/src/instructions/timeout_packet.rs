use crate::errors::RouterError;
use crate::router_cpi::on_timeout_packet_cpi;
use crate::router_cpi::{verify_non_membership_cpi, LightClientVerification};
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use ics25_handler::MembershipMsg;
use solana_ibc_types::events::{NoopEvent, TimeoutPacketEvent};
#[cfg(test)]
use solana_ibc_types::router::APP_STATE_SEED;

#[derive(Accounts)]
#[instruction(msg: MsgTimeoutPacket)]
pub struct TimeoutPacket<'info> {
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

pub fn timeout_packet(ctx: Context<TimeoutPacket>, msg: MsgTimeoutPacket) -> Result<()> {
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

    // Verify non-membership proof on counterparty chain via light client
    let client = &ctx.accounts.client;
    let light_client_verification = LightClientVerification {
        light_client_program: ctx.accounts.light_client_program.clone(),
        client_state: ctx.accounts.client_state.clone(),
        consensus_state: ctx.accounts.consensus_state.clone(),
    };

    let receipt_path =
        ics24::packet_receipt_commitment_path(&msg.packet.dest_client, msg.packet.sequence);

    // The proof from Cosmos is generated with path segments ["ibc", receipt_path]
    let non_membership_msg = MembershipMsg {
        height: msg.proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: msg.proof_timeout.clone(),
        path: vec![b"ibc".to_vec(), receipt_path],
        value: vec![], // Empty value for non-membership
    };

    let counterparty_timestamp =
        verify_non_membership_cpi(client, &light_client_verification, non_membership_msg)?;

    require!(
        counterparty_timestamp >= msg.packet.timeout_timestamp as u64,
        RouterError::InvalidTimeoutTimestamp
    );

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

    // CPI to IBC app's onTimeoutPacket
    on_timeout_packet_cpi(
        &ctx.accounts.ibc_app_program,
        &ctx.accounts.ibc_app_state,
        &ctx.accounts.router_program,
        &msg.packet,
        &msg.packet.payloads[0],
        &ctx.accounts.relayer.key(),
    )?;

    // Close the account and return rent to payer
    // TODO: Find more idiomatic way since we can't use auto close of anchor due to noop
    let dest_starting_lamports = ctx.accounts.payer.lamports();
    **ctx.accounts.payer.lamports.borrow_mut() = dest_starting_lamports
        .checked_add(packet_commitment_account.lamports())
        .ok_or(RouterError::ArithmeticOverflow)?;
    **packet_commitment_account.lamports.borrow_mut() = 0;

    let mut data = packet_commitment_account.try_borrow_mut_data()?;
    data.fill(0);

    emit!(TimeoutPacketEvent {
        client_id: msg.packet.source_client.clone(),
        sequence: msg.packet.sequence,
        packet: msg.packet,
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
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    struct TimeoutPacketTestContext {
        instruction: Instruction,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
        packet_commitment_pubkey: Pubkey,
        payer_pubkey: Pubkey,
        packet: Packet,
        dummy_app_state_pubkey: Pubkey,
    }

    struct TimeoutPacketTestParams {
        source_client_id: &'static str,
        dest_client_id: &'static str,
        port_id: &'static str,
        app_program_id: Option<Pubkey>,
        unauthorized_relayer: Option<Pubkey>,
        wrong_dest_client: Option<&'static str>,
        active_client: bool,
        initial_sequence: u64,
        timeout_timestamp: i64,
        proof_height: u64,
        with_existing_commitment: bool,
    }

    impl Default for TimeoutPacketTestParams {
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
                timeout_timestamp: 1000,
                proof_height: 100,
                with_existing_commitment: true,
            }
        }
    }

    fn setup_timeout_packet_test_with_params(
        params: TimeoutPacketTestParams,
    ) -> TimeoutPacketTestContext {
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
            params.timeout_timestamp,
        );

        let (packet_commitment_pda, _) = Pubkey::find_program_address(
            &[
                PACKET_COMMITMENT_SEED,
                packet.source_client.as_bytes(),
                &packet.sequence.to_le_bytes(),
            ],
            &crate::ID,
        );

        let msg = MsgTimeoutPacket {
            packet: packet.clone(),
            proof_timeout: vec![0u8; 32],
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
            data: crate::instruction::TimeoutPacket { msg }.data(),
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

        TimeoutPacketTestContext {
            instruction,
            accounts,
            packet_commitment_pubkey: packet_commitment_pda,
            payer_pubkey: payer,
            packet,
            dummy_app_state_pubkey: dummy_app_state_pda,
        }
    }

    #[test]
    fn test_timeout_packet_success() {
        let ctx = setup_timeout_packet_test_with_params(TimeoutPacketTestParams::default());

        let mollusk = setup_mollusk_with_mock_programs();

        // Get initial lamports for verification
        let initial_payer_lamports = ctx
            .accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &ctx.payer_pubkey)
            .map(|(_, account)| account.lamports)
            .unwrap();

        let commitment_lamports = ctx
            .accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &ctx.packet_commitment_pubkey)
            .map(|(_, account)| account.lamports)
            .unwrap();

        let checks = vec![
            Check::success(),
            // Verify packet commitment account is closed (0 lamports)
            Check::account(&ctx.packet_commitment_pubkey)
                .lamports(0)
                .build(),
            // Verify payer received the rent back
            Check::account(&ctx.payer_pubkey)
                .lamports(initial_payer_lamports + commitment_lamports)
                .build(),
        ];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);

        // Mock app doesn't track counters, so we just verify the instruction succeeded
    }

    #[test]
    fn test_timeout_packet_noop_no_commitment() {
        let ctx = setup_timeout_packet_test_with_params(TimeoutPacketTestParams {
            with_existing_commitment: false, // No packet commitment exists
            ..Default::default()
        });

        let mollusk = setup_mollusk_with_mock_programs();

        // When packet commitment doesn't exist, it should succeed (noop)
        let checks = vec![Check::success()];

        mollusk.process_and_validate_instruction(&ctx.instruction, &ctx.accounts, &checks);
    }

    #[test]
    fn test_timeout_packet_unauthorized_sender() {
        let ctx = setup_timeout_packet_test_with_params(TimeoutPacketTestParams {
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
    fn test_timeout_packet_invalid_counterparty() {
        let ctx = setup_timeout_packet_test_with_params(TimeoutPacketTestParams {
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
    fn test_timeout_packet_client_not_active() {
        let ctx = setup_timeout_packet_test_with_params(TimeoutPacketTestParams {
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
