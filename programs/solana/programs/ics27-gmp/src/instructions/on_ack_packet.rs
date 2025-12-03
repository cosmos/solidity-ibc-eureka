use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPCallAcknowledged;
use crate::state::{AccountVersion, CallResultStatus, GMPAppState, GMPCallResult, GMPCallResultAccount};
use anchor_lang::prelude::*;
use solana_ibc_proto::{GmpPacketData, ProstMessage, RawGmpPacketData};

/// Process IBC packet acknowledgement (called by router via CPI)
#[derive(Accounts)]
#[instruction(msg: solana_ibc_types::OnAcknowledgementPacketMsg)]
pub struct OnAckPacket<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump,
        constraint = !app_state.paused @ GMPError::AppPaused
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Router program calling this instruction
    pub router_program: Program<'info, ics26_router::program::Ics26Router>,

    /// Instructions sysvar for validating CPI caller
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + GMPCallResultAccount::INIT_SPACE,
        seeds = [GMPCallResult::SEED, msg.source_client.as_bytes(), &msg.sequence.to_le_bytes()],
        bump
    )]
    pub result_account: Account<'info, GMPCallResultAccount>,

    pub system_program: Program<'info, System>,
}

pub fn on_acknowledgement_packet(
    ctx: Context<OnAckPacket>,
    msg: solana_ibc_types::OnAcknowledgementPacketMsg,
) -> Result<()> {
    solana_ibc_types::validate_cpi_caller(
        &ctx.accounts.instruction_sysvar,
        &ctx.accounts.router_program.key(),
        &crate::ID,
    )
    .map_err(GMPError::from)?;

    let raw_packet = RawGmpPacketData::decode(msg.payload.value.as_slice())
        .map_err(|_| GMPError::InvalidPacketData)?;
    let packet_data =
        GmpPacketData::try_from(raw_packet).map_err(|_| GMPError::InvalidPacketData)?;

    let clock = Clock::get()?;
    let result = &mut ctx.accounts.result_account;
    result.version = AccountVersion::V1;
    result.sender = packet_data.sender.into_string();
    result.sequence = msg.sequence;
    result.source_client = msg.source_client.clone();
    result.dest_client = msg.dest_client;
    result.status = CallResultStatus::Acknowledged;
    result.acknowledgement = msg.acknowledgement.clone();
    result.result_timestamp = clock.unix_timestamp;
    result.bump = ctx.bumps.result_account;

    emit!(GMPCallAcknowledged {
        source_client: msg.source_client,
        sequence: msg.sequence,
        sender: result.sender.clone(),
        result_pda: result.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::constants::{GMP_PORT_ID, ICS27_ENCODING, ICS27_VERSION};
    use crate::state::GMPAppState;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    fn create_test_ack_msg() -> solana_ibc_types::OnAcknowledgementPacketMsg {
        solana_ibc_types::OnAcknowledgementPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: ICS27_VERSION.to_string(),
                encoding: ICS27_ENCODING.to_string(),
                value: vec![],
            },
            acknowledgement: vec![1, 2, 3],
            relayer: Pubkey::new_unique(),
        }
    }

    fn create_ack_instruction(
        app_state_pda: Pubkey,
        router_program: Pubkey,
        payer: Pubkey,
    ) -> Instruction {
        let instruction_data = crate::instruction::OnAcknowledgementPacket {
            msg: create_test_ack_msg(),
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    #[test]
    fn test_on_ack_packet_app_paused() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let router_program = ics26_router::ID;
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction = create_ack_instruction(app_state_pda, router_program, payer);

        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, true),
            create_router_program_account(router_program),
            create_instructions_sysvar_account_with_caller(router_program),
            create_authority_account(payer),
            create_system_program_account(),
        ];

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::AppPaused as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_ack_packet_invalid_app_state_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let router_program = ics26_router::ID;
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (_correct_app_state_pda, _correct_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, port_id.as_bytes()], &crate::ID);

        // Use wrong PDA
        let wrong_app_state_pda = Pubkey::new_unique();

        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: ICS27_VERSION.to_string(),
                encoding: ICS27_ENCODING.to_string(),
                value: vec![],
            },
            acknowledgement: vec![1, 2, 3],
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnAcknowledgementPacket { msg: ack_msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(wrong_app_state_pda, false), // Wrong PDA!
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Create account state at wrong PDA for testing
        let wrong_bump = 255u8;
        let accounts = vec![
            create_gmp_app_state_account(
                wrong_app_state_pda,
                wrong_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_instructions_sysvar_account_with_caller(router_program),
            create_authority_account(payer),
            create_system_program_account(),
        ];

        // Anchor ConstraintSeeds error (2006)
        let checks = vec![Check::err(ProgramError::Custom(2006))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_ack_packet_direct_call_rejected() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let router_program = ics26_router::ID;
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction = create_ack_instruction(app_state_pda, router_program, payer);

        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, false),
            create_router_program_account(router_program),
            create_instructions_sysvar_account_with_caller(crate::ID), // Direct call
            create_authority_account(payer),
            create_system_program_account(),
        ];

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::DirectCallNotAllowed as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_ack_packet_unauthorized_router() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let router_program = ics26_router::ID;
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction = create_ack_instruction(app_state_pda, router_program, payer);

        let unauthorized_program = Pubkey::new_unique();
        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, false),
            create_router_program_account(router_program),
            create_instructions_sysvar_account_with_caller(unauthorized_program), // Unauthorized
            create_authority_account(payer),
            create_system_program_account(),
        ];

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::UnauthorizedRouter as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_ack_packet_fake_sysvar_wormhole_attack() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let router_program = ics26_router::ID;
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let mut instruction = create_ack_instruction(app_state_pda, router_program, payer);

        // Simulate Wormhole attack: pass a completely different account with fake sysvar data
        let (fake_sysvar_pubkey, fake_sysvar_account) =
            create_fake_instructions_sysvar_account(router_program);

        // Modify the instruction to reference the fake sysvar (simulating attacker control)
        instruction.accounts[2] = AccountMeta::new_readonly(fake_sysvar_pubkey, false);

        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, false),
            create_router_program_account(router_program),
            // Wormhole attack: provide a DIFFERENT account instead of the real sysvar
            (fake_sysvar_pubkey, fake_sysvar_account),
            create_authority_account(payer),
            create_system_program_account(),
        ];

        // Should be rejected by Anchor's address constraint check
        let checks = vec![Check::err(ProgramError::Custom(
            anchor_lang::error::ErrorCode::ConstraintAddress as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
