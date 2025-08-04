use crate::errors::RouterError;
use crate::instructions::light_client_cpi::{verify_non_membership_cpi, LightClientVerification};
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use solana_light_client_interface::MembershipMsg;

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

    let non_membership_msg = MembershipMsg {
        height: msg.proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: msg.proof_timeout.clone(),
        path: vec![receipt_path],
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

    // TODO: CPI to IBC app's onTimeoutPacket

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
        packet_data: msg.packet.try_to_vec()?,
    });

    Ok(())
}

#[event]
pub struct TimeoutPacketEvent {
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
    use solana_sdk::system_program;

    #[test]
    fn test_timeout_packet_unauthorized_sender() {
        let authority = Pubkey::new_unique();
        let unauthorized_relayer = Pubkey::new_unique(); // Different from authority
        let payer = unauthorized_relayer;
        let source_client_id = "source-client";
        let dest_client_id = "dest-client";
        let port_id = "test-port";
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            source_client_id,
            authority,
            light_client_program,
            dest_client_id,
            true,
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());

        let packet = create_test_packet(
            1,
            source_client_id,
            dest_client_id,
            port_id,
            "dest-port",
            1000,
        );

        let (packet_commitment_pda, packet_commitment_data) =
            setup_packet_commitment(source_client_id, packet.sequence, &packet);

        let msg = MsgTimeoutPacket {
            packet,
            proof_timeout: vec![0u8; 32],
            proof_height: 100,
        };

        let instruction_data = crate::instruction::TimeoutPacket { msg };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(packet_commitment_pda, false),
                AccountMeta::new_readonly(unauthorized_relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
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
            create_account(packet_commitment_pda, packet_commitment_data, crate::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
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
    fn test_timeout_packet_invalid_counterparty() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let payer = authority;
        let source_client_id = "source-client";
        let port_id = "test-port";
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        // Client expects counterparty "expected-dest-client"
        let (client_pda, client_data) = setup_client(
            source_client_id,
            authority,
            light_client_program,
            "expected-dest-client",
            true,
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());

        // But packet is destined for "wrong-dest-client"
        let packet = create_test_packet(
            1,
            source_client_id,
            "wrong-dest-client",
            port_id,
            "dest-port",
            1000,
        );

        let (packet_commitment_pda, packet_commitment_data) =
            setup_packet_commitment(source_client_id, packet.sequence, &packet);

        let msg = MsgTimeoutPacket {
            packet,
            proof_timeout: vec![0u8; 32],
            proof_height: 100,
        };

        let instruction_data = crate::instruction::TimeoutPacket { msg };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(packet_commitment_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
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
            create_account(packet_commitment_pda, packet_commitment_data, crate::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
            create_account(client_pda, client_data, crate::ID),
            create_program_account(light_client_program),
            create_account(client_state, vec![0u8; 100], light_client_program),
            create_account(consensus_state, vec![0u8; 100], light_client_program),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidCounterpartyClient as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_timeout_packet_client_not_active() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let payer = authority;
        let source_client_id = "source-client";
        let dest_client_id = "dest-client";
        let port_id = "test-port";
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        // Create inactive client
        let (client_pda, client_data) = setup_client(
            source_client_id,
            authority,
            light_client_program,
            dest_client_id,
            false, // Client is not active
        );
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());

        let packet = create_test_packet(
            1,
            source_client_id,
            dest_client_id,
            port_id,
            "dest-port",
            1000,
        );

        let (packet_commitment_pda, packet_commitment_data) =
            setup_packet_commitment(source_client_id, packet.sequence, &packet);

        let msg = MsgTimeoutPacket {
            packet,
            proof_timeout: vec![0u8; 32],
            proof_height: 100,
        };

        let instruction_data = crate::instruction::TimeoutPacket { msg };

        let client_state = Pubkey::new_unique();
        let consensus_state = Pubkey::new_unique();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(ibc_app_pda, false),
                AccountMeta::new(packet_commitment_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
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
            create_account(packet_commitment_pda, packet_commitment_data, crate::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
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
