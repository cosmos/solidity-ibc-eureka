use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPTimeoutProcessed;
use crate::state::GMPAppState;
use anchor_lang::prelude::*;

/// Process IBC packet timeout (called by router via CPI)
#[derive(Accounts)]
#[instruction(msg: solana_ibc_types::OnTimeoutPacketMsg)]
pub struct OnTimeoutPacket<'info> {
    /// App state account - PDA validation done in handler since `port_id` comes from router message
    #[account()]
    pub app_state: Account<'info, GMPAppState>,

    /// Router program calling this instruction
    /// CHECK: Validated in handler
    pub router_program: UncheckedAccount<'info>,

    /// Relayer fee payer (passed by router but not used in timeout handler)
    /// CHECK: Router always passes this account
    #[account(mut)]
    pub payer: UncheckedAccount<'info>,

    /// CHECK: System program (passed by router but not used in timeout handler)
    pub system_program: UncheckedAccount<'info>,
}

pub fn on_timeout_packet(
    ctx: Context<OnTimeoutPacket>,
    msg: solana_ibc_types::OnTimeoutPacketMsg,
) -> Result<()> {
    // Get clock directly via syscall
    let clock = Clock::get()?;

    // Validate app_state PDA using port_id from state
    let (expected_app_state_pda, _bump) = Pubkey::find_program_address(
        &[
            GMP_APP_STATE_SEED,
            ctx.accounts.app_state.port_id.as_bytes(),
        ],
        ctx.program_id,
    );
    require!(
        ctx.accounts.app_state.key() == expected_app_state_pda,
        GMPError::InvalidAccountAddress
    );

    let app_state = &ctx.accounts.app_state;

    // Validate router program
    require!(
        ctx.accounts.router_program.key() == app_state.router_program,
        GMPError::UnauthorizedRouter
    );

    // Check if app is operational
    app_state.can_operate()?;

    // Parse packet data and timeout info from router message
    let packet_data = crate::router_cpi::parse_timeout_data_from_router_cpi(&msg)?;
    let sequence = msg.sequence;

    // Extract timeout info from router message
    let timeout_info = extract_timeout_info_from_router_msg(&msg);

    // Validate packet data
    packet_data.validate()?;

    // Convert cross-chain sender address to deterministic Solana pubkey
    let sender = crate::utils::derive_pubkey_from_address(&packet_data.sender)?;

    emit!(GMPTimeoutProcessed {
        sender,
        sequence,
        timeout_info: format_timeout_info(&timeout_info),
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "GMP call timed out: sender={}, sequence={}, timeout={}",
        sender,
        sequence,
        format_timeout_info(&timeout_info)
    );

    Ok(())
}

/// Extract timeout info from router message
const fn extract_timeout_info_from_router_msg(
    router_msg: &solana_ibc_types::OnTimeoutPacketMsg,
) -> TimeoutInfo {
    // The OnTimeoutPacketMsg currently doesn't include explicit timeout info
    // In IBC v2/Eureka, timeout information would be part of the packet metadata
    // that the router processes. Since the router already validated the timeout
    // condition before calling us, we can infer this was a valid timeout.

    // We can use the sequence number and current context to provide meaningful info
    // For now, we'll use the sequence as a proxy for timeout ordering
    // In a full implementation, the router would provide more timeout details

    // Extract sequence as timestamp (simplified approach)
    // Real implementation would need timeout height/timestamp from router context
    TimeoutInfo::Timestamp(router_msg.sequence as i64)
}

/// Timeout information structure
#[derive(Debug, Clone)]
pub enum TimeoutInfo {
    Height(u64),
    Timestamp(i64),
    Both { height: u64, timestamp: i64 },
}

/// Format timeout info for display/logging
fn format_timeout_info(timeout_info: &TimeoutInfo) -> String {
    match timeout_info {
        TimeoutInfo::Height(height) => format!("height:{height}"),
        TimeoutInfo::Timestamp(timestamp) => format!("timestamp:{timestamp}"),
        TimeoutInfo::Both { height, timestamp } => {
            format!("height:{height},timestamp:{timestamp}")
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    #[test]
    fn test_on_timeout_packet_app_paused() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let timeout_msg = solana_ibc_types::OnTimeoutPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: port_id.clone(),
                dest_port: port_id.clone(),
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
                value: vec![],
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnTimeoutPacket { msg: timeout_msg };

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
                port_id,
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
            "OnTimeoutPacket should fail when app is paused"
        );
    }

    #[test]
    fn test_on_timeout_packet_unauthorized_router() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let wrong_router = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let timeout_msg = solana_ibc_types::OnTimeoutPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: port_id.clone(),
                dest_port: port_id.clone(),
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
                value: vec![],
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnTimeoutPacket { msg: timeout_msg };

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
                port_id,
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
            "OnTimeoutPacket should fail with unauthorized router"
        );
    }

    #[test]
    fn test_on_timeout_packet_invalid_app_state_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (_correct_app_state_pda, _correct_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        // Use wrong PDA
        let wrong_app_state_pda = Pubkey::new_unique();

        let timeout_msg = solana_ibc_types::OnTimeoutPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: port_id.clone(),
                dest_port: port_id.clone(),
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
                value: vec![],
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnTimeoutPacket { msg: timeout_msg };

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
                port_id,
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
            "OnTimeoutPacket should fail with invalid app_state PDA"
        );
    }

    #[test]
    fn test_on_timeout_packet_invalid_packet_data() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        // Create timeout message with invalid packet data in payload.value
        let timeout_msg = solana_ibc_types::OnTimeoutPacketMsg {
            source_client: "cosmoshub-1".to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: port_id.clone(),
                dest_port: port_id.clone(),
                version: "gmp-1".to_string(),
                encoding: "proto3".to_string(),
                value: vec![0xFF, 0xFF, 0xFF], // Invalid/malformed packet data!
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnTimeoutPacket { msg: timeout_msg };

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
                port_id,
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
            "OnTimeoutPacket should fail with invalid/malformed packet data"
        );
    }
}
