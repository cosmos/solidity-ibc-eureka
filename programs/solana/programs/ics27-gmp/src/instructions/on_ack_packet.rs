use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPAcknowledgementProcessed;
use crate::state::GMPAppState;
use anchor_lang::prelude::*;

/// Process IBC packet acknowledgement (called by router via CPI)
#[derive(Accounts)]
#[instruction(msg: solana_ibc_types::OnAcknowledgementPacketMsg)]
pub struct OnAckPacket<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump,
        has_one = router_program @ GMPError::UnauthorizedRouter
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Router program calling this instruction
    /// Validated via `has_one` constraint on `app_state`
    pub router_program: UncheckedAccount<'info>,

    /// Relayer fee payer (passed by router but not used in acknowledgement handler)
    /// CHECK: Router always passes this account
    #[account(mut)]
    pub payer: UncheckedAccount<'info>,

    /// System program (passed by router but not used in acknowledgement handler)
    pub system_program: Program<'info, System>,
}

pub fn on_acknowledgement_packet(
    ctx: Context<OnAckPacket>,
    msg: solana_ibc_types::OnAcknowledgementPacketMsg,
) -> Result<()> {
    let clock = Clock::get()?;
    let app_state = &ctx.accounts.app_state;

    // Check if app is operational
    app_state.can_operate()?;

    // Parse packet data and acknowledgement from router message
    let (packet_data, acknowledgement) = crate::router_cpi::parse_ack_data_from_router_cpi(&msg)?;
    let sequence = msg.sequence;

    // Validate packet data
    packet_data.validate()?;

    // Convert cross-chain sender address to deterministic Solana pubkey
    let sender = crate::utils::derive_pubkey_from_address(&packet_data.sender)?;

    // Determine if acknowledgement indicates success
    let ack_success = is_acknowledgement_success(&acknowledgement);

    emit!(GMPAcknowledgementProcessed {
        sender,
        sequence,
        ack_success,
        timestamp: clock.unix_timestamp,
    });

    if ack_success {
        msg!(
            "GMP call acknowledged successfully: sender={}, sequence={}",
            sender,
            sequence
        );
    } else {
        msg!(
            "GMP call acknowledged with error: sender={}, sequence={}",
            sender,
            sequence
        );
    }

    Ok(())
}

/// Determine if acknowledgement indicates success
fn is_acknowledgement_success(acknowledgement: &[u8]) -> bool {
    // Parse the acknowledgement to determine success/failure
    // This depends on the specific acknowledgement format used

    if acknowledgement.is_empty() {
        return false;
    }

    // Check for universal error acknowledgement
    if acknowledgement == ACK_ERROR {
        return false;
    }

    // Try to parse as GMPAcknowledgement
    // If we can't parse it, assume it's a success (non-empty, non-error)
    crate::state::GMPAcknowledgement::try_from_slice(acknowledgement)
        .map(|ack| ack.success)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use crate::constants::GMP_PORT_ID;
    use crate::state::GMPAppState;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    #[test]
    fn test_on_ack_packet_app_paused() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
                value: vec![],
            },
            acknowledgement: vec![1, 2, 3],
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnAcknowledgementPacket { msg: ack_msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                true, // paused
            ),
            create_router_program_account(router_program),
            create_authority_account(payer),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnAckPacket should fail when app is paused"
        );
    }

    #[test]
    fn test_on_ack_packet_unauthorized_router() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let wrong_router = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
                value: vec![],
            },
            acknowledgement: vec![1, 2, 3],
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnAcknowledgementPacket { msg: ack_msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new_readonly(wrong_router, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_router_program_account(wrong_router),
            create_authority_account(payer),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnAckPacket should fail with unauthorized router"
        );
    }

    #[test]
    fn test_on_ack_packet_invalid_app_state_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
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
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
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
                router_program,
                authority,
                wrong_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_authority_account(payer),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnAckPacket should fail with invalid app_state PDA"
        );
    }

    #[test]
    fn test_on_ack_packet_invalid_packet_data() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        // Create acknowledgement message with invalid packet data in payload.value
        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
                value: vec![0xFF, 0xFF, 0xFF], // Invalid/malformed packet data!
            },
            acknowledgement: vec![1, 2, 3],
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnAcknowledgementPacket { msg: ack_msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_authority_account(payer),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnAckPacket should fail with invalid/malformed packet data"
        );
    }
}
