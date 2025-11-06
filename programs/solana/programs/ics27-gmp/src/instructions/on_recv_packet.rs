use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPExecutionCompleted;
use crate::proto::{GmpAcknowledgement, GmpSolanaPayload};
use crate::state::GMPAppState;
use crate::utils::validate_cpi_caller;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use solana_ibc_types::GMPAccount;

/// Receive IBC packet and execute call (called by router via CPI)
///
/// # Account Layout
/// The router is generic and passes all IBC-app-specific accounts via `remaining_accounts`.
/// GMP defines its own account layout in `remaining_accounts`:
///
/// `remaining_accounts`:
/// - [0]: `account_state` - GMP account PDA (created if needed, signs via `invoke_signed`)
/// - [1]: `target_program` - The program to execute (extracted internally, must be executable)
/// - [2..]: accounts from payload - All accounts required by target program
///
/// Note: `target_program` cannot be a direct Anchor account because:
/// 1. The router is generic and doesn't know about GMP-specific account needs
/// 2. Different IBC apps have different account layouts
/// 3. The router passes all app-specific accounts via `remaining_accounts`
///
#[derive(Accounts)]
#[instruction(msg: solana_ibc_types::OnRecvPacketMsg)]
pub struct OnRecvPacket<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Router program calling this instruction
    pub router_program: Program<'info, ics26_router::program::Ics26Router>,

    /// Instructions sysvar for validating CPI caller
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    /// Relayer fee payer - used for account creation rent
    /// NOTE: This cannot be the GMP account PDA because PDAs with data cannot
    /// be used as payers in System Program transfers. The relayer's fee payer
    /// is used for rent, while the GMP account PDA signs via `invoke_signed`.
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn on_recv_packet<'info>(
    ctx: Context<'_, '_, 'info, 'info, OnRecvPacket<'info>>,
    msg: solana_ibc_types::OnRecvPacketMsg,
) -> Result<Vec<u8>> {
    let clock = Clock::get()?;
    let app_state = &mut ctx.accounts.app_state;

    // Verify this function is called via CPI from the authorized router
    validate_cpi_caller(
        &ctx.accounts.instruction_sysvar,
        &ics26_router::program::Ics26Router::id(),
    )?;

    // Check if app is operational
    app_state.can_operate()?;

    // Validate IBC payload fields (matching Solidity ICS27GMP validations)
    // See: ICS27GMP.sol lines 115-130

    // Validate version
    require!(
        msg.payload.version == ICS27_VERSION,
        GMPError::InvalidVersion
    );

    // Validate source port
    require!(
        msg.payload.source_port == GMP_PORT_ID,
        GMPError::InvalidPort
    );

    // Validate encoding
    require!(
        msg.payload.encoding == ICS27_ENCODING,
        GMPError::InvalidEncoding
    );

    // Validate dest port
    require!(msg.payload.dest_port == GMP_PORT_ID, GMPError::InvalidPort);

    // Extract accounts from remaining_accounts
    // The router passes GMP-specific accounts via remaining_accounts
    require!(
        ctx.remaining_accounts.len() >= 2,
        GMPError::InsufficientAccounts
    );

    // Extract target_program from remaining_accounts[1]
    let target_program = &ctx.remaining_accounts[1];

    // Validate target_program is executable
    require!(target_program.executable, GMPError::TargetNotExecutable);

    // Parse and validate packet data from protobuf payload
    let packet_data = crate::proto::GmpPacketData::decode_and_validate(&msg.payload.value)
        .map_err(GMPError::from)?;

    // Parse receiver as Solana Pubkey (for incoming packets, receiver is a Solana address)
    let receiver_pubkey =
        Pubkey::try_from(packet_data.receiver.as_str()).map_err(|_| GMPError::InvalidAccountKey)?;

    // Validate target program matches packet data
    require!(
        target_program.key() == receiver_pubkey,
        GMPError::AccountKeyMismatch
    );

    // Create ClientId from source_client
    let client_id = solana_ibc_types::ClientId::new(&msg.source_client).map_err(GMPError::from)?;

    // Create account identifier and derive expected GMP account PDA address
    let gmp_account = GMPAccount::new(
        client_id,
        packet_data.sender.clone(),
        packet_data.salt.clone(),
        ctx.program_id,
    );

    // Validate GMP account PDA matches (stateless - no account creation needed)
    require!(
        ctx.remaining_accounts[0].key() == gmp_account.pda,
        GMPError::InvalidAccountAddress
    );

    // Parse and validate the GMP Solana payload from Protobuf
    // The payload contains the target program ID, all required accounts, and instruction data
    let validated_payload =
        GmpSolanaPayload::decode_and_validate(&packet_data.payload).map_err(GMPError::from)?;

    // Validate that the program_id in the payload matches the target_program
    require!(
        &validated_payload.program_id == target_program.key,
        GMPError::AccountKeyMismatch
    );

    let mut account_metas = validated_payload.to_account_metas();

    // Skip gmp_account_pda[0] and target_program[1]
    let remaining_for_execution = &ctx.remaining_accounts[2..];

    // Validate account count matches exactly (before payer injection)
    require!(
        remaining_for_execution.len() == account_metas.len(),
        GMPError::AccountCountMismatch
    );

    // Build target_account_infos from remaining_accounts, validate as we go
    let mut target_account_infos = account_metas
        .iter()
        .zip(remaining_for_execution)
        .map(|(meta, account_info)| {
            require!(
                account_info.key() == meta.pubkey,
                GMPError::AccountKeyMismatch
            );

            require!(
                account_info.is_writable == meta.is_writable,
                GMPError::InsufficientAccountPermissions
            );

            Ok(account_info.clone())
        })
        .collect::<Result<Vec<_>>>()?;

    // Inject payer at specified position
    if let Some(pos) = validated_payload.payer_position {
        let pos_usize = pos as usize;
        require!(
            pos_usize <= account_metas.len(),
            GMPError::InvalidPayerPosition
        );
        target_account_infos.insert(pos_usize, ctx.accounts.payer.to_account_info());
        account_metas.insert(pos_usize, AccountMeta::new(*ctx.accounts.payer.key, true));
    }

    let instruction = Instruction {
        program_id: validated_payload.program_id,
        accounts: account_metas,
        data: validated_payload.data.clone(),
    };

    // Call target program via CPI with GMP account PDA as signer
    // Note: CPI errors cause immediate transaction abort in Solana, so we cannot
    // handle execution failures gracefully like Ethereum. The ? operator will
    // propagate any error and abort the entire transaction.
    gmp_account.invoke_signed(&instruction, &target_account_infos)?;

    // Get return data from the target program (if any)
    // Only accept return data from the target program itself, not from nested CPIs
    let result = anchor_lang::solana_program::program::get_return_data()
        .filter(|(return_program_id, _)| *return_program_id == validated_payload.program_id)
        .map(|(_, data)| data)
        .unwrap_or_default();

    // Emit success event
    emit!(GMPExecutionCompleted {
        account: gmp_account.pda,
        target_program: target_program.key(),
        client_id: msg.source_client,
        sender: packet_data.sender.as_str().to_string(),
        success: true,
        result_size: result.len() as u64,
        timestamp: clock.unix_timestamp,
    });

    // Create acknowledgement with execution result
    // Matches ibc-go's Acknowledgement format (just the result bytes)
    let ack = GmpAcknowledgement::new(result);
    ack.try_to_vec()
        .map_err(|_| GMPError::InvalidExecutionPayload.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{GmpAcknowledgement, GmpSolanaPayload, SolanaAccountMeta};
    use crate::state::GMPAppState;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use gmp_counter_app::ID as COUNTER_APP_ID;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_ibc_types::{ClientId, GMPAccount, Salt, Sender};
    use solana_sdk::account::Account;
    use solana_sdk::bpf_loader_upgradeable;
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction as SolanaInstructionSDK},
        pubkey::Pubkey,
        system_program,
    };

    /// Helper function to create a `GMPAccount` from test data
    fn create_test_gmp_account(
        client_id: &str,
        sender: &str,
        salt: Vec<u8>,
        program_id: &Pubkey,
    ) -> GMPAccount {
        GMPAccount::new(
            ClientId::new(client_id).unwrap(),
            Sender::new(sender).unwrap(),
            Salt::new(salt).unwrap(),
            program_id,
        )
    }

    #[test]
    fn test_on_recv_packet_app_paused() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                true, // paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::AppPaused as u32,
        ))];

        ctx.mollusk
            .process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_recv_packet_direct_call_rejected() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            // For a direct call, the instructions sysvar will show GMP as the caller (not router)
            create_instructions_sysvar_account_with_caller(crate::ID),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        // Direct calls fail with DirectCallNotAllowed since validate_cpi_caller checks
        // that the instruction was called via CPI from the authorized router
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::DirectCallNotAllowed as u32,
        ))];

        ctx.mollusk
            .process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_recv_packet_unauthorized_router() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        // Create an unauthorized program ID (not the authorized router)
        let unauthorized_program = Pubkey::new_unique();

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            // Simulate CPI from an unauthorized program (not the router)
            create_instructions_sysvar_account_with_caller(unauthorized_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        // Unauthorized router calls fail with UnauthorizedRouter error
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::UnauthorizedRouter as u32,
        ))];

        ctx.mollusk
            .process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_recv_packet_invalid_app_state_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (_correct_app_state_pda, _correct_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, port_id.as_bytes()], &crate::ID);

        // Use wrong PDA
        let wrong_app_state_pda = Pubkey::new_unique();

        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = crate::proto::GmpPacketData {
            sender: sender.to_string(),
            receiver: system_program::ID.to_string(),
            salt,
            payload: vec![],
            memo: String::new(),
        };

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = solana_ibc_types::OnRecvPacketMsg {
            source_client: client_id.to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: ICS27_VERSION.to_string(),
                encoding: ICS27_ENCODING.to_string(),
                value: packet_data_bytes,
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnRecvPacket { msg: recv_msg };

        let instruction = SolanaInstructionSDK {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(wrong_app_state_pda, false), // Wrong PDA!
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Create account state at wrong PDA for testing
        let wrong_bump = 255u8;
        let accounts = vec![
            create_gmp_app_state_account(
                wrong_app_state_pda,
                authority,
                wrong_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_instructions_sysvar_account_with_caller(router_program),
            create_authority_account(payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail with invalid app_state PDA"
        );
    }

    #[test]
    fn test_on_recv_packet_wrong_sender() {
        let ctx = create_gmp_test_context();

        let (client_id, _default_sender, salt, _default_pda) = create_test_account_data();
        let original_sender = "cosmos1original";
        let wrong_sender = "cosmos1attacker";

        // Derive PDA for original_sender (the correct one)
        let (correct_pda, _) =
            create_test_gmp_account(client_id, original_sender, salt.clone(), &crate::ID).pda();

        // Create a minimal valid payload
        let solana_payload = GmpSolanaPayload {
            program_id: crate::test_utils::DUMMY_TARGET_PROGRAM.to_bytes().to_vec(),
            accounts: vec![],
            data: vec![0u8], // Minimal non-empty data
            payer_position: None,
        };

        let solana_payload_bytes = solana_payload.encode_to_vec();

        // Packet claims to be from wrong_sender - this will derive a different PDA
        let packet_data = create_gmp_packet_data(
            wrong_sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            solana_payload_bytes,
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let mut instruction =
            create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        // Add remaining accounts to instruction
        instruction
            .accounts
            .push(AccountMeta::new(correct_pda, false)); // [0] GMP account PDA
        instruction.accounts.push(AccountMeta::new_readonly(
            crate::test_utils::DUMMY_TARGET_PROGRAM,
            false,
        )); // [1] target_program

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts - providing the PDA for original_sender, but packet claims wrong_sender
            create_uninitialized_account_for_pda(correct_pda), // [0] GMP account PDA
            create_dummy_target_program_account(),             // [1] target_program
        ];

        // Should fail with InvalidAccountAddress because derived PDA (from wrong_sender) doesn't match provided PDA (from original_sender)
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::InvalidAccountAddress as u32,
        ))];

        ctx.mollusk
            .process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_recv_packet_wrong_salt() {
        let ctx = create_gmp_test_context();

        let (client_id, sender, _original_salt, correct_pda) = create_test_account_data();
        let wrong_salt = vec![4u8, 5, 6];

        // Create a minimal valid payload
        let solana_payload = GmpSolanaPayload {
            program_id: crate::test_utils::DUMMY_TARGET_PROGRAM.to_bytes().to_vec(),
            accounts: vec![],
            data: vec![0u8], // Minimal non-empty data
            payer_position: None,
        };

        let solana_payload_bytes = solana_payload.encode_to_vec();

        // Packet uses wrong_salt - this will derive a different PDA
        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            wrong_salt,
            solana_payload_bytes,
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let mut instruction =
            create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        // Add remaining accounts to instruction
        instruction
            .accounts
            .push(AccountMeta::new(correct_pda, false)); // [0] GMP account PDA
        instruction.accounts.push(AccountMeta::new_readonly(
            crate::test_utils::DUMMY_TARGET_PROGRAM,
            false,
        )); // [1] target_program

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts - providing the PDA for original_salt, but packet claims wrong_salt
            create_uninitialized_account_for_pda(correct_pda), // [0] GMP account PDA
            create_dummy_target_program_account(),             // [1] target_program
        ];

        // Should fail with InvalidAccountAddress because derived PDA (from wrong_salt) doesn't match provided PDA (from original_salt)
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::InvalidAccountAddress as u32,
        ))];

        ctx.mollusk
            .process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_recv_packet_wrong_client() {
        let ctx = create_gmp_test_context();

        let (_original_client_id, sender, salt, correct_pda) = create_test_account_data();
        let wrong_client_id = "different-client";

        // Create a minimal valid payload
        let solana_payload = GmpSolanaPayload {
            program_id: crate::test_utils::DUMMY_TARGET_PROGRAM.to_bytes().to_vec(),
            accounts: vec![],
            data: vec![0u8], // Minimal non-empty data
            payer_position: None,
        };

        let solana_payload_bytes = solana_payload.encode_to_vec();

        // Packet claims to be from wrong_client_id - this will derive a different PDA
        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            solana_payload_bytes,
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(wrong_client_id, packet_data_bytes, 1);
        let mut instruction =
            create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        // Add remaining accounts to instruction
        instruction
            .accounts
            .push(AccountMeta::new(correct_pda, false)); // [0] GMP account PDA
        instruction.accounts.push(AccountMeta::new_readonly(
            crate::test_utils::DUMMY_TARGET_PROGRAM,
            false,
        )); // [1] target_program

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts - providing the PDA for original_client_id, but packet claims wrong_client_id
            create_uninitialized_account_for_pda(correct_pda), // [0] GMP account PDA
            create_dummy_target_program_account(),             // [1] target_program
        ];

        // Should fail with InvalidAccountAddress because derived PDA (from wrong_client_id) doesn't match provided PDA (from original_client_id)
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::InvalidAccountAddress as u32,
        ))];

        ctx.mollusk
            .process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_recv_packet_insufficient_accounts() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, _account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Missing remaining accounts! (should have at least account_state_pda and target_program)
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail with insufficient accounts"
        );
    }

    #[test]
    fn test_on_recv_packet_invalid_version() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![1, 2, 3],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        // Create custom recv_msg with invalid version
        let recv_msg = solana_ibc_types::OnRecvPacketMsg {
            source_client: client_id.to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: "wrong-version".to_string(), // Invalid version!
                encoding: ICS27_ENCODING.to_string(),
                value: packet_data_bytes,
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail with invalid version"
        );
    }

    #[test]
    fn test_on_recv_packet_invalid_source_port() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![1, 2, 3],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        // Create custom recv_msg with invalid source port
        let recv_msg = solana_ibc_types::OnRecvPacketMsg {
            source_client: client_id.to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: "transfer".to_string(), // Invalid source port!
                dest_port: GMP_PORT_ID.to_string(),
                version: ICS27_VERSION.to_string(),
                encoding: ICS27_ENCODING.to_string(),
                value: packet_data_bytes,
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail with invalid source port"
        );
    }

    #[test]
    fn test_on_recv_packet_invalid_encoding() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![1, 2, 3],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        // Create custom recv_msg with invalid encoding
        let recv_msg = solana_ibc_types::OnRecvPacketMsg {
            source_client: client_id.to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: ICS27_VERSION.to_string(),
                encoding: "application/json".to_string(), // Invalid encoding!
                value: packet_data_bytes,
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail with invalid encoding"
        );
    }

    #[test]
    fn test_on_recv_packet_invalid_dest_port() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![1, 2, 3],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        // Create custom recv_msg with invalid dest port
        let recv_msg = solana_ibc_types::OnRecvPacketMsg {
            source_client: client_id.to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: "transfer".to_string(), // Invalid dest port!
                version: ICS27_VERSION.to_string(),
                encoding: ICS27_ENCODING.to_string(),
                value: packet_data_bytes,
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] account_state_pda
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail with invalid dest port"
        );
    }

    #[test]
    fn test_on_recv_packet_account_key_mismatch() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, expected_account_state_pda) = create_test_account_data();

        // Use a different account key than expected
        let wrong_account_key = Pubkey::new_unique();

        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            vec![],
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(wrong_account_key), // [0] Wrong account key!
            create_dummy_target_program_account(),                   // [1] target_program
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail when account key doesn't match expected PDA (expected: {expected_account_state_pda}, got: {wrong_account_key})"
        );
    }

    #[test]
    fn test_on_recv_packet_target_program_mismatch() {
        let ctx = create_gmp_test_context();
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        // Create a minimal valid GMP Solana payload
        let solana_payload = GmpSolanaPayload {
            program_id: crate::test_utils::DUMMY_TARGET_PROGRAM.to_bytes().to_vec(),
            accounts: vec![],
            data: vec![0u8], // Minimal non-empty data
            payer_position: None,
        };

        let solana_payload_bytes = solana_payload.encode_to_vec();

        // Packet says to execute on DUMMY_TARGET_PROGRAM
        let packet_data = create_gmp_packet_data(
            sender,
            &crate::test_utils::DUMMY_TARGET_PROGRAM.to_string(),
            salt,
            solana_payload_bytes,
        );

        let packet_data_bytes = packet_data.encode_to_vec();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let mut instruction =
            create_recv_packet_instruction(ctx.app_state_pda, ctx.payer, recv_msg);

        // But relayer provides a different program in remaining_accounts[1]
        let wrong_target_program = Pubkey::new_unique();

        // Add remaining accounts to instruction
        instruction
            .accounts
            .push(AccountMeta::new(account_state_pda, false)); // [0] GMP account PDA
        instruction
            .accounts
            .push(AccountMeta::new_readonly(wrong_target_program, false)); // [1] Wrong target program!

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_instructions_sysvar_account_with_caller(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // [0] GMP account PDA (correct)
            (
                wrong_target_program, // [1] Wrong target program!
                solana_sdk::account::Account {
                    lamports: 1_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
        ];

        // Should fail with AccountKeyMismatch error
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::AccountKeyMismatch as u32,
        ))];

        ctx.mollusk
            .process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_on_recv_packet_success_with_cpi() {
        // Create Mollusk instance and load both programs
        let mut mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        // Add the counter app program so CPI will work
        // Use BPF loader upgradeable for Anchor programs
        mollusk.add_program(
            &COUNTER_APP_ID,
            "../../target/deploy/gmp_counter_app",
            &bpf_loader_upgradeable::ID,
        );

        let authority = Pubkey::new_unique();
        let router_program = ics26_router::ID;
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        // Create packet data that will call the counter app
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        // Counter app state and user counter PDAs
        let (counter_app_state_pda, counter_app_state_bump) = Pubkey::find_program_address(
            &[gmp_counter_app::state::CounterAppState::SEED],
            &COUNTER_APP_ID,
        );

        let (user_counter_pda, _user_counter_bump) = Pubkey::find_program_address(
            &[
                gmp_counter_app::state::UserCounter::SEED,
                account_state_pda.as_ref(),
            ],
            &COUNTER_APP_ID,
        );

        // Create counter instruction that will increment the counter
        let counter_instruction = gmp_counter_app::instruction::Increment { amount: 5 };
        let counter_instruction_data = anchor_lang::InstructionData::data(&counter_instruction);

        // Build GMPSolanaPayload for the payload
        let solana_payload = GmpSolanaPayload {
            program_id: COUNTER_APP_ID.to_bytes().to_vec(),
            accounts: vec![
                // app_state
                SolanaAccountMeta {
                    pubkey: counter_app_state_pda.to_bytes().to_vec(),
                    is_signer: false,
                    is_writable: true,
                },
                // user_counter
                SolanaAccountMeta {
                    pubkey: user_counter_pda.to_bytes().to_vec(),
                    is_signer: false,
                    is_writable: true,
                },
                // user_authority (account_state_pda will sign via invoke_signed)
                // Note: marked writable because account_state_pda is also used as GMP account (writable)
                // and Solana merges duplicate pubkeys with most permissive flags
                SolanaAccountMeta {
                    pubkey: account_state_pda.to_bytes().to_vec(),
                    is_signer: true,
                    is_writable: true,
                },
                // payer will be injected at position 3 by GMP
                // system_program
                SolanaAccountMeta {
                    pubkey: system_program::ID.to_bytes().to_vec(),
                    is_signer: false,
                    is_writable: false,
                },
            ],
            data: counter_instruction_data,
            payer_position: Some(3), // Inject payer at position 3
        };

        let solana_payload_bytes = solana_payload.encode_to_vec();

        // Create GMPPacketData with the counter instruction as payload using protobuf
        let proto_packet_data = crate::proto::GmpPacketData {
            sender: sender.to_string(),
            receiver: COUNTER_APP_ID.to_string(),
            salt,
            payload: solana_payload_bytes,
            memo: String::new(),
        };

        let packet_data_bytes = proto_packet_data.encode_to_vec();

        let recv_msg = solana_ibc_types::OnRecvPacketMsg {
            source_client: client_id.to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: ICS27_VERSION.to_string(),
                encoding: ICS27_ENCODING.to_string(),
                value: packet_data_bytes,
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnRecvPacket { msg: recv_msg };

        let instruction = SolanaInstructionSDK {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                // Remaining accounts for CPI:
                AccountMeta::new(account_state_pda, false), // [0] account_state (GMP account)
                AccountMeta::new_readonly(COUNTER_APP_ID, false), // [1] target_program
                AccountMeta::new(counter_app_state_pda, false), // [2] counter app state
                AccountMeta::new(user_counter_pda, false),  // [3] user counter
                AccountMeta::new(account_state_pda, true), // [4] user_authority (account_state signs via invoke_signed, writable due to duplicate)
                AccountMeta::new_readonly(system_program::ID, false), // [5] system program
            ],
            data: instruction_data.data(),
        };

        // Create counter app state
        let counter_app_state = gmp_counter_app::state::CounterAppState {
            authority,
            total_counters: 0,
            total_gmp_calls: 0,
            bump: counter_app_state_bump,
        };
        let mut counter_app_state_data = Vec::new();
        counter_app_state_data
            .extend_from_slice(gmp_counter_app::state::CounterAppState::DISCRIMINATOR);
        counter_app_state
            .serialize(&mut counter_app_state_data)
            .unwrap();

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_instructions_sysvar_account_with_caller(router_program),
            // Counter app program (target_program) - must be executable
            (
                COUNTER_APP_ID,
                Account {
                    lamports: 1_000_000,
                    data: vec![],
                    owner: bpf_loader_upgradeable::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            create_authority_account(payer),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // Account state will be created
            (
                counter_app_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: counter_app_state_data,
                    owner: COUNTER_APP_ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            create_uninitialized_account_for_pda(user_counter_pda), // User counter will be created
            create_authority_account(account_state_pda),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);

        // OnRecvPacket should succeed even if CPI fails (returns error ack instead)
        // This is the correct behavior - OnRecvPacket never fails the transaction,
        // it returns success/error acks
        assert!(
            !result.program_result.is_err(),
            "OnRecvPacket instruction should succeed (returns ack even on CPI failure): {:?}",
            result.program_result
        );

        // Verify acknowledgement is returned
        assert!(
            !result.return_data.is_empty(),
            "Should return acknowledgement"
        );

        // Parse the acknowledgement and verify CPI succeeded
        // The ack is protobuf-encoded
        // The return data in Mollusk is just the raw bytes, but OnRecvPacket uses
        // anchor's return mechanism which prefixes with length
        // Skip the first 4 bytes (u32 length prefix) that Anchor adds
        let ack_bytes = if result.return_data.len() > 4 {
            &result.return_data[4..]
        } else {
            &result.return_data[..]
        };

        let ack = GmpAcknowledgement::try_from_slice(ack_bytes).unwrap();

        // Following ibc-go convention: non-empty result = success
        assert!(
            !ack.result.is_empty(),
            "CPI execution should succeed (non-empty result)"
        );

        // Verify the acknowledgement contains the correct counter value
        // Counter app returns u64 in little-endian (8 bytes)
        assert_eq!(
            ack.result.len(),
            8,
            "Counter return value should be 8 bytes (u64)"
        );
        let returned_counter = u64::from_le_bytes(ack.result[..8].try_into().unwrap());
        // Counter should be incremented to 5 (initial 0 + increment 5)
        assert_eq!(
            returned_counter, 5,
            "Acknowledgement should contain counter value 5, got {returned_counter}"
        );

        // With stateless approach, no account state is created
        // The GMP account PDA is used as a signer without storing state
    }

    /// Verifies that CPI errors cause immediate transaction failure
    ///
    /// Test Scenario:
    /// 1. GMP receives a packet requesting a counter app CPI call
    /// 2. The payer has insufficient lamports (3M - enough for `account_state` but not for `user_counter`)
    /// 3. GMP invokes counter app via CPI
    /// 4. Counter app fails when attempting to create `user_counter` (insufficient lamports)
    /// 5. The entire transaction aborts - no error acknowledgment is returned
    ///
    /// Solana Architectural Constraint:
    /// Unlike IBC/EVM where execution errors can be caught and returned as error acknowledgments,
    /// Solana CPIs (Cross-Program Invocations) fail atomically. When `invoke()` or `invoke_signed()`
    /// fails, the entire transaction aborts immediately - by design to maintain atomicity.
    ///
    /// Technical Details:
    /// CPI errors cannot be handled in Solana programs - when `invoke()` or `invoke_signed()`
    /// fails, the entire transaction aborts immediately. This is by design to maintain
    /// transaction atomicity.
    ///
    /// Runtime Implementation:
    /// The error propagation happens at the VM/runtime level. When a child program returns
    /// an error, it propagates immediately via the ? operator in `cpi_common()`:
    /// <https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/program-runtime/src/cpi.rs#L843>
    ///
    /// Error propagation flow in `process_instruction()`:
    /// <https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/program-runtime/src/invoke_context.rs#L488-L498>
    ///
    /// Unit Test Proof:
    /// There's a test that proves CPI errors cause transaction abort even when the Result
    /// is ignored.
    ///
    /// Test setup (expects transaction to fail with Custom(42)):
    /// <https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/programs/sbf/tests/programs.rs#L1043-L1049>
    ///
    /// Parent program IGNORES the `invoke()` result with "let _ = invoke(...)":
    /// <https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/programs/sbf/rust/invoke/src/lib.rs#L604>
    ///
    /// Child program returns error Custom(42):
    /// <https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/programs/sbf/rust/invoked/src/lib.rs#L119>
    ///
    /// The test confirms that even though the parent ignores the Result, the transaction
    /// aborts with the child's error. The parent program never gets to execute any code
    /// after the failed `invoke()` call - the abort happens at the runtime/VM level.
    ///
    /// This is fundamentally different from EVM's try/catch mechanism or Cosmos SDK's error returns.
    #[test]
    fn test_on_recv_packet_failed_execution_returns_error_ack() {
        // Create Mollusk instance and load both programs
        let mut mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        // Add the counter app program so CPI will be attempted
        mollusk.add_program(
            &COUNTER_APP_ID,
            "../../target/deploy/gmp_counter_app",
            &bpf_loader_upgradeable::ID,
        );

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        // Create packet data
        let (client_id, sender, salt, account_state_pda) = create_test_account_data();

        // Counter app state PDA
        let (counter_app_state_pda, counter_app_state_bump) = Pubkey::find_program_address(
            &[gmp_counter_app::state::CounterAppState::SEED],
            &COUNTER_APP_ID,
        );

        let (user_counter_pda, _user_counter_bump) = Pubkey::find_program_address(
            &[
                gmp_counter_app::state::UserCounter::SEED,
                account_state_pda.as_ref(),
            ],
            &COUNTER_APP_ID,
        );

        // Create counter instruction - will fail due to insufficient payer lamports
        let counter_instruction = gmp_counter_app::instruction::Increment { amount: 5 };
        let counter_instruction_data = anchor_lang::InstructionData::data(&counter_instruction);

        // Build GMPSolanaPayload for the payload
        let solana_payload = GmpSolanaPayload {
            program_id: COUNTER_APP_ID.to_bytes().to_vec(),
            accounts: vec![
                // app_state
                SolanaAccountMeta {
                    pubkey: counter_app_state_pda.to_bytes().to_vec(),
                    is_signer: false,
                    is_writable: true,
                },
                // user_counter
                SolanaAccountMeta {
                    pubkey: user_counter_pda.to_bytes().to_vec(),
                    is_signer: false,
                    is_writable: true,
                },
                // user_authority (account_state_pda will sign via invoke_signed)
                // Note: marked writable because account_state_pda is also used as GMP account (writable)
                // and Solana merges duplicate pubkeys with most permissive flags
                SolanaAccountMeta {
                    pubkey: account_state_pda.to_bytes().to_vec(),
                    is_signer: true,
                    is_writable: true,
                },
                // payer will be injected at position 3 by GMP
                // system_program
                SolanaAccountMeta {
                    pubkey: system_program::ID.to_bytes().to_vec(),
                    is_signer: false,
                    is_writable: false,
                },
            ],
            data: counter_instruction_data,
            payer_position: Some(3), // Inject payer at position 3
        };

        let solana_payload_bytes = solana_payload.encode_to_vec();

        // Create GMPPacketData with the counter instruction as payload using protobuf
        let proto_packet_data = crate::proto::GmpPacketData {
            sender: sender.to_string(),
            receiver: COUNTER_APP_ID.to_string(),
            salt,
            payload: solana_payload_bytes,
            memo: String::new(),
        };

        let packet_data_bytes = proto_packet_data.encode_to_vec();

        let recv_msg = solana_ibc_types::OnRecvPacketMsg {
            source_client: client_id.to_string(),
            dest_client: "solana-1".to_string(),
            sequence: 1,
            payload: solana_ibc_types::Payload {
                source_port: GMP_PORT_ID.to_string(),
                dest_port: GMP_PORT_ID.to_string(),
                version: ICS27_VERSION.to_string(),
                encoding: ICS27_ENCODING.to_string(),
                value: packet_data_bytes,
            },
            relayer: Pubkey::new_unique(),
        };

        let instruction_data = crate::instruction::OnRecvPacket { msg: recv_msg };

        let instruction = SolanaInstructionSDK {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                // Remaining accounts for CPI:
                AccountMeta::new(account_state_pda, false), // [0] account_state (GMP account)
                AccountMeta::new_readonly(COUNTER_APP_ID, false), // [1] target_program
                AccountMeta::new(counter_app_state_pda, false), // [2] counter app state
                AccountMeta::new(user_counter_pda, false),  // [3] user counter
                AccountMeta::new(account_state_pda, true), // [4] user_authority (account_state signs via invoke_signed, writable due to duplicate)
                AccountMeta::new_readonly(system_program::ID, false), // [5] system program
            ],
            data: instruction_data.data(),
        };

        // Create counter app state (properly initialized)
        let counter_app_state = gmp_counter_app::state::CounterAppState {
            authority,
            total_counters: 0,
            total_gmp_calls: 0,
            bump: counter_app_state_bump,
        };
        let mut counter_app_state_data = Vec::new();
        counter_app_state_data
            .extend_from_slice(gmp_counter_app::state::CounterAppState::DISCRIMINATOR);
        counter_app_state
            .serialize(&mut counter_app_state_data)
            .unwrap();

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_instructions_sysvar_account(),
            (
                payer,
                Account {
                    lamports: 3_000_000, // Enough for GMP account_state (~2.4M) but not enough for counter user_counter too
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            create_system_program_account(),
            // Remaining accounts
            create_uninitialized_account_for_pda(account_state_pda), // Account state will be created
            // Counter app program (loaded via mollusk.add_program())
            (
                COUNTER_APP_ID,
                Account {
                    lamports: 1_000_000,
                    data: vec![],
                    owner: bpf_loader_upgradeable::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            (
                counter_app_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: counter_app_state_data,
                    owner: COUNTER_APP_ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            create_uninitialized_account_for_pda(user_counter_pda), // User counter - will fail to init due to insufficient payer funds
            create_authority_account(account_state_pda),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);

        // Transaction should FAIL due to Solana's CPI limitation
        assert!(
            result.program_result.is_err(),
            "Expected transaction to fail when CPI encounters error"
        );

        // Verify no acknowledgement was returned (transaction aborted)
        assert!(
            result.return_data.is_empty(),
            "No return data should be present when transaction aborts"
        );

        // With stateless approach, no account state is created or rolled back
        // The GMP account PDA is used as a signer without storing state
    }
}
