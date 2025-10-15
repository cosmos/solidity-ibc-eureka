use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPAcknowledgementProcessed;
use crate::state::{GMPAcknowledgementExt, GMPAppState, GMPPacketData};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;

/// Standard discriminator for GMP acknowledgment callbacks
/// This serves as a standardized interface identifier similar to ERC165
const GMP_ACK_CALLBACK_DISCRIMINATOR: [u8; 8] = [0x4A, 0x69, 0x62, 0x63, 0x41, 0x63, 0x6B, 0x00]; // "JibcAck\0"

/// Process IBC packet acknowledgement (called by router via CPI)
#[derive(Accounts)]
#[instruction(msg: solana_ibc_types::OnAcknowledgementPacketMsg)]
pub struct OnAckPacket<'info> {
    /// App state account - PDA validation done in handler since `port_id` comes from router message
    #[account()]
    pub app_state: Account<'info, GMPAppState>,

    /// Router program calling this instruction
    /// CHECK: Validated in handler
    pub router_program: UncheckedAccount<'info>,

    /// Relayer fee payer (passed by router but not used in acknowledgement handler)
    /// CHECK: Router always passes this account
    #[account(mut)]
    pub payer: UncheckedAccount<'info>,

    /// CHECK: System program (passed by router but not used in acknowledgement handler)
    pub system_program: UncheckedAccount<'info>,
}

pub fn on_acknowledgement_packet(
    ctx: Context<OnAckPacket>,
    msg: solana_ibc_types::OnAcknowledgementPacketMsg,
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

    // Parse packet data and acknowledgement from router message
    let (packet_data, acknowledgement) = crate::router_cpi::parse_ack_data_from_router_cpi(&msg)?;
    let sequence = msg.sequence;

    // Validate packet data
    packet_data.validate()?;

    // Convert cross-chain sender address to deterministic Solana pubkey
    let sender = crate::utils::derive_pubkey_from_address(&packet_data.sender)?;

    // Determine if acknowledgement indicates success
    let ack_success = is_acknowledgement_success(&acknowledgement);

    // Attempt callback to original sender if they implement callback interface
    // Note: We handle callback failures gracefully to avoid breaking packet processing
    if let Err(callback_error) = attempt_ack_callback(
        &sender,
        ack_success,
        &packet_data,
        &acknowledgement,
        sequence,
        ctx.remaining_accounts,
    ) {
        // Handle callback errors gracefully - check error code
        let error_string = format!("{callback_error:?}");
        if error_string.contains("CallbackInterfaceNotSupported") {
            msg!("Sender does not support callbacks, continuing without callback");
        } else if error_string.contains("InvalidCallbackAccount") {
            msg!("Invalid callback account provided, continuing without callback");
        } else if error_string.contains("CallbackExecutionFailed") {
            msg!("Callback execution failed, continuing packet processing");
        } else {
            msg!("Callback failed with error: {}", error_string);
        }
    }

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
    if acknowledgement == b"error" {
        return false;
    }

    // Try to parse as GMPAcknowledgement
    if let Ok(ack) = crate::state::GMPAcknowledgement::try_from_slice(acknowledgement) {
        ack.success
    } else {
        // If we can't parse it, assume it's a success (non-empty, non-error)
        true
    }
}

/// Attempt to make a callback to the original sender for acknowledgment processing
/// This is similar to Ethereum's IBCSenderCallbacksLib.ackPacketCallback
fn attempt_ack_callback(
    sender: &Pubkey,
    success: bool,
    packet_data: &GMPPacketData,
    acknowledgement: &[u8],
    sequence: u64,
    remaining_accounts: &[AccountInfo],
) -> Result<()> {
    // We need the sender program account to make the callback
    // This would typically be passed as an additional remaining account
    if remaining_accounts.is_empty() {
        msg!("No callback account provided, skipping callback");
        return Ok(());
    }

    let callback_account = &remaining_accounts[0];

    // Verify the callback account matches the sender
    if callback_account.key() != *sender {
        msg!(
            "Callback account key mismatch: expected {}, got {}",
            sender,
            callback_account.key()
        );
        return Err(GMPError::InvalidCallbackAccount.into());
    }

    // Enhanced interface detection for callback support
    if !supports_gmp_callbacks(callback_account) {
        msg!("Sender program does not support GMP callbacks");
        return Err(GMPError::CallbackInterfaceNotSupported.into());
    }

    // Create standardized callback instruction data
    let callback_data = create_ack_callback_data(success, packet_data, acknowledgement, sequence)?;

    // Create callback instruction with proper discriminator for acknowledgment callbacks
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&GMP_ACK_CALLBACK_DISCRIMINATOR);
    instruction_data.extend_from_slice(&callback_data);

    let callback_instruction = anchor_lang::solana_program::instruction::Instruction {
        program_id: *sender,
        accounts: vec![],
        data: instruction_data,
    };

    // Attempt the callback with proper error handling
    match invoke(
        &callback_instruction,
        std::slice::from_ref(callback_account),
    ) {
        Ok(()) => {
            msg!(
                "Successfully executed acknowledgment callback for sender: {}",
                sender
            );
        }
        Err(e) => {
            msg!(
                "Acknowledgment callback execution failed for sender {}: {:?}",
                sender,
                e
            );
            return Err(GMPError::CallbackExecutionFailed.into());
        }
    }

    Ok(())
}

/// Create callback data for acknowledgment
/// Matches Ethereum's `OnAcknowledgementPacketCallback` structure
fn create_ack_callback_data(
    success: bool,
    packet_data: &GMPPacketData,
    acknowledgement: &[u8],
    sequence: u64,
) -> Result<Vec<u8>> {
    // Standard IBC callback data structure matching Ethereum implementation
    #[derive(AnchorSerialize)]
    struct PayloadData {
        source_port: String,
        dest_port: String,
        version: String,
        encoding: String,
        value: Vec<u8>,
    }

    #[derive(AnchorSerialize)]
    struct OnAcknowledgementPacketCallback {
        source_client: String,
        destination_client: String,
        sequence: u64,
        payload: PayloadData,
        acknowledgement: Vec<u8>,
        relayer: [u8; 32], // Pubkey as 32 bytes
        success: bool,     // Additional field for Solana convenience
    }

    let callback_data = OnAcknowledgementPacketCallback {
        source_client: packet_data.client_id.clone(),
        destination_client: "solana-client".to_string(), // Default for now
        sequence,
        payload: PayloadData {
            source_port: "gmp".to_string(),
            dest_port: "gmp".to_string(),
            version: "gmp-1".to_string(),
            encoding: "proto3".to_string(),
            value: packet_data.payload.clone(),
        },
        acknowledgement: acknowledgement.to_vec(),
        relayer: [0u8; 32], // Default relayer for now
        success,
    };

    callback_data
        .try_to_vec()
        .map_err(|_| GMPError::CallbackDataSerializationFailed.into())
}

/// Check if a program supports GMP callbacks
/// This is a simplified version of ERC165 interface detection for Solana
/// TODO: Add more robust detection
fn supports_gmp_callbacks(program_account: &AccountInfo) -> bool {
    // Check if account is executable (i.e., a program)
    if !program_account.executable {
        return false;
    }

    if program_account.data_len() == 0 {
        return false;
    }
    true
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
    fn test_on_ack_packet_app_paused() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
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
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
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

        let (_correct_app_state_pda, _correct_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        // Use wrong PDA
        let wrong_app_state_pda = Pubkey::new_unique();

        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
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
            "OnAckPacket should fail with invalid app_state PDA"
        );
    }

    #[test]
    fn test_on_ack_packet_invalid_packet_data() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        // Create acknowledgement message with invalid packet data in payload.value
        let ack_msg = solana_ibc_types::OnAcknowledgementPacketMsg {
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
            "OnAckPacket should fail with invalid/malformed packet data"
        );
    }
}
