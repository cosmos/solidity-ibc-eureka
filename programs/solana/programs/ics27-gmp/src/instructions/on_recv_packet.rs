use crate::constants::*;
use crate::errors::GMPError;
use crate::events::{GMPAccountCreated, GMPExecutionCompleted, GMPExecutionFailed};
use crate::state::{
    AccountState, GMPAcknowledgement, GMPAppState, GMPPacketData, SolanaInstruction,
};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{hash::hash, instruction::Instruction, program::invoke_signed};

const EXECUTION_SUCCESS_RESULT: &[u8] = b"execution_success";

/// Receive IBC packet and execute call (called by router via CPI)
#[derive(Accounts)]
#[instruction(msg: solana_ibc_types::OnRecvPacketMsg)]
pub struct OnRecvPacket<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump,
        has_one = router_program @ GMPError::UnauthorizedRouter
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Router program calling this instruction
    /// Validated via `has_one` constraint on `app_state`
    pub router_program: UncheckedAccount<'info>,

    /// Relayer fee payer - used for account creation rent
    /// NOTE: This cannot be the GMP account PDA because PDAs with data cannot
    /// be used as payers in System Program transfers. The relayer's fee payer
    /// is used for rent, while the GMP account PDA signs via `invoke_signed`.
    /// CHECK: Validated for sufficient funds when account creation is needed
    #[account(mut)]
    pub payer: UncheckedAccount<'info>,

    /// System program (passed by router)
    pub system_program: Program<'info, System>,
    // Additional accounts (accessed via remaining_accounts from relayer):
    // - [0]: account_state - GMP account PDA (created if needed, signs via invoke_signed)
    // - [1]: target_program - Target program to execute
    // - [2+]: accounts from payload - All accounts required by target program
}

pub fn on_recv_packet<'info>(
    ctx: Context<'_, '_, '_, 'info, OnRecvPacket<'info>>,
    msg: solana_ibc_types::OnRecvPacketMsg,
) -> Result<Vec<u8>> {
    let clock = Clock::get()?;
    let current_time = clock.unix_timestamp;
    let app_state = &mut ctx.accounts.app_state;

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

    // Parse packet data from router message
    let packet_data = crate::router_cpi::parse_packet_data_from_router_cpi(&msg)?;

    // Validate packet data
    packet_data.validate()?;

    // Get account and target program from remaining accounts
    // The router passes these as the first two remaining accounts
    require!(
        ctx.remaining_accounts.len() >= 2,
        GMPError::InsufficientAccounts
    );

    // Work around lifetime issues by accessing items inline
    if ctx.remaining_accounts.len() < 2 {
        return Err(GMPError::InsufficientAccounts.into());
    }

    // Parse receiver as Solana Pubkey (for incoming packets, receiver is a Solana address)
    let receiver_pubkey =
        Pubkey::try_from(packet_data.receiver.as_str()).map_err(|_| GMPError::InvalidAccountKey)?;

    // Validate target program matches packet data
    require!(
        ctx.remaining_accounts[1].key() == receiver_pubkey,
        GMPError::AccountKeyMismatch
    );

    // Derive expected account address
    let (expected_account_address, _bump) = AccountState::derive_address(
        &packet_data.client_id,
        &packet_data.sender,
        &packet_data.salt,
        ctx.program_id,
    )?;

    // Validate account info matches derived address
    require!(
        ctx.remaining_accounts[0].key() == expected_account_address,
        GMPError::InvalidAccountAddress
    );

    // Validate account ownership (security check)
    if !ctx.remaining_accounts[0].data_is_empty() {
        crate::utils::validate_account_ownership(&ctx.remaining_accounts[0], ctx.program_id)?;
    }

    // Get or create account state using utility function
    let account_info = &ctx.remaining_accounts[0];
    let payer_account_info = ctx.accounts.payer.to_account_info();
    let system_program_account_info = ctx.accounts.system_program.to_account_info();
    let (mut account_state, is_new_account) = crate::utils::get_or_create_account(
        account_info,
        &packet_data.client_id,
        &packet_data.sender,
        &packet_data.salt,
        &payer_account_info,
        &system_program_account_info,
        ctx.program_id,
        current_time,
        _bump,
    )?;

    // Validate execution authority
    validate_execution_authority(&account_state, &packet_data)?;

    // Parse the SolanaInstruction from Protobuf payload
    // The payload contains the target program ID, all required accounts, and instruction data
    let solana_instruction = SolanaInstruction::try_from_slice(&packet_data.payload)?;
    solana_instruction.validate()?;

    // Prepare signer seeds for invoke_signed
    // The GMP account PDA will sign for the target program execution
    // NOTE: Sender is hashed to fit Solana's 32-byte PDA seed constraint
    let sender_hash = hash(account_state.sender.as_bytes()).to_bytes();
    let client_id_bytes = account_state.client_id.as_bytes().to_vec();
    let salt_bytes = account_state.salt.clone();
    let bump = account_state.bump;

    // Build signer seeds on stack for invoke_signed
    let bump_array = [bump];
    let signer_seeds: &[&[u8]] = &[
        AccountState::SEED, // b"gmp_account"
        &client_id_bytes,   // Source chain client ID
        &sender_hash,       // Hashed sender address (32 bytes)
        &salt_bytes,        // User-provided salt
        &bump_array,        // PDA bump seed
    ];

    // Execute with nonce protection and record result
    let execution_result = execute_with_nonce_protection(&mut account_state, current_time, || {
        execute_target_program(
            &solana_instruction,
            ctx.remaining_accounts,
            signer_seeds,
            &ctx.accounts.payer,
        )
    });

    // Save account state using utility function
    let account_info = &ctx.remaining_accounts[0];
    crate::utils::save_account_state(account_info, &account_state)?;

    // Emit event for new accounts
    if is_new_account {
        emit!(GMPAccountCreated {
            account: ctx.remaining_accounts[0].key(),
            client_id: packet_data.client_id.clone(),
            sender: packet_data.sender.clone(),
            salt: packet_data.salt.clone(),
            created_at: current_time,
        });
    }

    // Handle execution result and create acknowledgement
    match execution_result {
        Ok(result) => {
            emit!(GMPExecutionCompleted {
                account: ctx.remaining_accounts[0].key(),
                target_program: ctx.remaining_accounts[1].key(),
                client_id: packet_data.client_id.clone(),
                sender: packet_data.sender.clone(),
                nonce: account_state.nonce,
                success: true,
                result_size: result.len() as u64,
                timestamp: current_time,
            });

            let ack = GMPAcknowledgement::success(result);
            Ok(ack.try_to_vec()?)
        }
        Err(e) => {
            let error_msg = format!("Execution failed: {e:?}");

            emit!(GMPExecutionFailed {
                account: ctx.remaining_accounts[0].key(),
                target_program: ctx.remaining_accounts[1].key(),
                error_code: 0, // Simplified error code handling
                error_message: error_msg.clone(),
                timestamp: current_time,
            });

            // Return error acknowledgement instead of failing transaction
            // This matches Ethereum behavior
            let ack = GMPAcknowledgement::error(error_msg);
            Ok(ack.try_to_vec()?)
        }
    }
}

/// Validate that the execution is authorized for this GMP account
/// Only the original creator (same `client_id`, sender, and salt) can execute via this account
///
/// Note: This function is public primarily for testing purposes
pub fn validate_execution_authority(
    account_state: &AccountState,
    packet_data: &GMPPacketData,
) -> Result<()> {
    // Ensure the packet comes from the same source chain client
    require!(
        account_state.client_id == packet_data.client_id,
        GMPError::WrongCounterpartyClient
    );

    // Ensure the packet is from the original sender who created this account
    require!(
        account_state.sender == packet_data.sender,
        GMPError::UnauthorizedSender
    );

    // Ensure the salt matches (for deterministic PDA derivation)
    require!(
        account_state.salt == packet_data.salt,
        GMPError::InvalidSalt
    );

    Ok(())
}

/// Execute with nonce protection and error handling
/// Increments the nonce before execution to prevent replay attacks
///
/// Note: This function is public primarily for testing purposes
pub fn execute_with_nonce_protection<F, T>(
    account_state: &mut AccountState,
    current_time: i64,
    execution_fn: F,
) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    // Increment nonce BEFORE execution to prevent replay attacks
    // Even if execution fails, the nonce is incremented
    account_state.execute_nonce_increment(current_time);

    // Execute the target program
    execution_fn()
}

/// Execute target program with GMP account PDA as signer
fn execute_target_program<'a>(
    solana_instruction: &SolanaInstruction,
    remaining_accounts: &[AccountInfo<'a>],
    signer_seeds: &[&[u8]],
    payer: &AccountInfo<'a>,
) -> Result<Vec<u8>> {
    let program_id = solana_instruction.get_program_id()?;
    let mut account_metas = solana_instruction.to_account_metas()?;

    let payer_position = inject_payer_if_needed(
        &mut account_metas,
        solana_instruction.payer_position,
        payer.key,
    )?;

    let target_account_infos =
        map_and_validate_accounts(&account_metas, remaining_accounts, payer_position, payer)?;

    let instruction = Instruction {
        program_id,
        accounts: account_metas,
        data: solana_instruction.data.clone(),
    };

    invoke_signed(&instruction, &target_account_infos, &[signer_seeds])
        .map(|()| EXECUTION_SUCCESS_RESULT.to_vec())
        .map_err(|_| GMPError::TargetExecutionFailed.into())
}

/// Inject payer into account metas if specified
///
/// Note: This function is public primarily for testing purposes
pub fn inject_payer_if_needed(
    account_metas: &mut Vec<AccountMeta>,
    payer_position: Option<u32>,
    payer_key: &Pubkey,
) -> Result<Option<usize>> {
    match payer_position {
        None => Ok(None),
        Some(pos) if (pos as usize) <= account_metas.len() => {
            let pos_usize = pos as usize;
            account_metas.insert(pos_usize, AccountMeta::new(*payer_key, true));
            Ok(Some(pos_usize))
        }
        _ => Err(GMPError::InvalidPayerPosition.into()),
    }
}

/// Calculate offset for account mapping based on payer injection position
///
/// Note: This function is public primarily for testing purposes
pub const fn calculate_account_offset(
    account_index: usize,
    payer_position: Option<usize>,
    base_offset: usize,
) -> usize {
    match payer_position {
        Some(pos) if account_index > pos => base_offset - 1,
        _ => base_offset,
    }
}

/// Map account metas to account infos and validate permissions
fn map_and_validate_accounts<'a>(
    account_metas: &[AccountMeta],
    remaining_accounts: &[AccountInfo<'a>],
    payer_position: Option<usize>,
    payer: &AccountInfo<'a>,
) -> Result<Vec<AccountInfo<'a>>> {
    const TARGET_ACCOUNTS_OFFSET: usize = 2;

    require!(
        account_metas.len() + TARGET_ACCOUNTS_OFFSET <= remaining_accounts.len() + 1,
        GMPError::InsufficientAccounts
    );

    let mut target_account_infos = Vec::new();

    for (i, meta) in account_metas.iter().enumerate() {
        let account_info = if Some(i) == payer_position {
            payer
        } else {
            let offset = calculate_account_offset(i, payer_position, TARGET_ACCOUNTS_OFFSET);
            &remaining_accounts[i + offset]
        };

        require!(
            account_info.key() == meta.pubkey,
            GMPError::AccountKeyMismatch
        );

        if meta.is_writable && !account_info.is_writable {
            return Err(GMPError::InsufficientAccountPermissions.into());
        }

        target_account_infos.push(account_info.clone());
    }

    Ok(target_account_infos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        AccountState, GMPAppState, GMPPacketData, SolanaAccountMeta, SolanaInstruction,
    };
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction as SolanaInstructionSDK},
        pubkey::Pubkey,
        system_program,
    };

    #[test]
    fn test_validate_execution_authority_success() {
        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = b"salt";

        let (_account_pda, bump) =
            AccountState::derive_address(client_id, sender, salt, &crate::ID).unwrap();

        let account_state = AccountState {
            client_id: client_id.to_string(),
            sender: sender.to_string(),
            salt: salt.to_vec(),
            nonce: 0,
            created_at: 1_600_000_000,
            last_executed_at: 1_600_000_000,
            execution_count: 0,
            bump,
        };

        let packet_data = GMPPacketData {
            client_id: client_id.to_string(),
            sender: sender.to_string(),
            receiver: Pubkey::new_unique().to_string(),
            salt: salt.to_vec(),
            payload: vec![1, 2, 3],
            memo: String::new(),
        };

        assert!(validate_execution_authority(&account_state, &packet_data).is_ok());
    }

    #[test]
    fn test_validate_execution_authority_wrong_client() {
        let (_account_pda, bump) =
            AccountState::derive_address("cosmoshub-1", "cosmos1test", b"", &crate::ID).unwrap();

        let account_state = AccountState {
            client_id: "cosmoshub-1".to_string(),
            sender: "cosmos1test".to_string(),
            salt: vec![],
            nonce: 0,
            created_at: 1_600_000_000,
            last_executed_at: 1_600_000_000,
            execution_count: 0,
            bump,
        };

        let packet_data = GMPPacketData {
            client_id: "different-client".to_string(),
            sender: "cosmos1test".to_string(),
            receiver: Pubkey::new_unique().to_string(),
            salt: vec![],
            payload: vec![1, 2, 3],
            memo: String::new(),
        };

        assert!(validate_execution_authority(&account_state, &packet_data).is_err());
    }

    #[test]
    fn test_execute_with_nonce_protection_increments_on_success() {
        let (_account_pda, bump) =
            AccountState::derive_address("cosmoshub-1", "cosmos1test", b"", &crate::ID).unwrap();

        let mut account_state = AccountState {
            client_id: "cosmoshub-1".to_string(),
            sender: "cosmos1test".to_string(),
            salt: vec![],
            nonce: 5,
            created_at: 1_600_000_000,
            last_executed_at: 1_600_000_000,
            execution_count: 10,
            bump,
        };

        let current_time = 1_700_000_000;
        let old_nonce = account_state.nonce;

        let result = execute_with_nonce_protection(&mut account_state, current_time, || {
            Ok("success".to_string())
        });

        assert!(result.is_ok());
        assert_eq!(account_state.nonce, old_nonce + 1);
    }

    #[test]
    fn test_inject_payer_at_beginning() {
        let payer_key = Pubkey::new_unique();
        let mut account_metas = vec![
            AccountMeta::new(Pubkey::new_unique(), false),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
        ];

        let result = inject_payer_if_needed(&mut account_metas, Some(0), &payer_key);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(0));
        assert_eq!(account_metas.len(), 3);
        assert_eq!(account_metas[0].pubkey, payer_key);
    }

    #[test]
    fn test_inject_payer_none() {
        let payer_key = Pubkey::new_unique();
        let mut account_metas = vec![
            AccountMeta::new(Pubkey::new_unique(), false),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
        ];

        let original_len = account_metas.len();
        let result = inject_payer_if_needed(&mut account_metas, None, &payer_key);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
        assert_eq!(account_metas.len(), original_len);
    }

    #[test]
    fn test_calculate_account_offset_no_payer() {
        assert_eq!(calculate_account_offset(0, None, 2), 2);
        assert_eq!(calculate_account_offset(5, None, 2), 2);
    }

    #[test]
    fn test_calculate_account_offset_with_payer_after() {
        assert_eq!(calculate_account_offset(3, Some(2), 2), 1);
        assert_eq!(calculate_account_offset(4, Some(2), 2), 1);
    }

    #[test]
    fn test_on_recv_packet_app_paused() {
        let ctx = create_gmp_test_context();

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                true, // paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(account_state_pda),
            create_system_program_account(),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_on_recv_packet_frozen_account() {
        let ctx = create_gmp_test_context();

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt.clone(), vec![]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_account_state(
                account_state_pda,
                client_id.to_string(),
                sender.to_string(),
                salt,
                account_bump,
            ),
            create_system_program_account(),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_on_recv_packet_unauthorized_router() {
        let ctx = create_gmp_test_context();
        let wrong_router = Pubkey::new_unique();

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction =
            create_recv_packet_instruction(ctx.app_state_pda, wrong_router, ctx.payer, recv_msg);

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program, // State has correct router
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(wrong_router),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(account_state_pda),
            create_system_program_account(),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail with unauthorized router"
        );
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

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data = GMPPacketData {
            client_id: client_id.to_string(),
            sender: sender.to_string(),
            receiver: system_program::ID.to_string(),
            salt,
            payload: vec![],
            memo: String::new(),
        };

        let packet_data_bytes = packet_data.try_to_vec().unwrap();

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
                router_program,
                authority,
                wrong_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_authority_account(payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(account_state_pda),
            create_system_program_account(),
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

        let client_id = "cosmoshub-1";
        let original_sender = "cosmos1original";
        let wrong_sender = "cosmos1attacker";
        let salt = vec![1u8, 2, 3];

        // Account was created by original_sender
        let (account_state_pda, account_bump) =
            AccountState::derive_address(client_id, original_sender, &salt, &crate::ID).unwrap();

        // Packet claims to be from wrong_sender
        let packet_data = create_gmp_packet_data(
            client_id,
            wrong_sender,
            system_program::ID,
            salt.clone(),
            vec![],
        );
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_account_state(
                account_state_pda,
                client_id.to_string(),
                original_sender.to_string(), // Account owned by original sender
                salt,
                account_bump,
            ),
            create_system_program_account(),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail when sender doesn't match account owner"
        );
    }

    #[test]
    fn test_on_recv_packet_wrong_salt() {
        let ctx = create_gmp_test_context();

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let original_salt = vec![1u8, 2, 3];
        let wrong_salt = vec![4u8, 5, 6];

        // Account was created with original_salt
        let (account_state_pda, account_bump) =
            AccountState::derive_address(client_id, sender, &original_salt, &crate::ID).unwrap();

        // Packet uses wrong_salt
        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, wrong_salt, vec![]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_account_state(
                account_state_pda,
                client_id.to_string(),
                sender.to_string(),
                original_salt, // Account has original salt
                account_bump,
            ),
            create_system_program_account(),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail when salt doesn't match"
        );
    }

    #[test]
    fn test_on_recv_packet_insufficient_accounts() {
        let ctx = create_gmp_test_context();

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            // Missing remaining accounts!
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

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![1, 2, 3]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

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

        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(account_state_pda),
            create_system_program_account(),
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

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![1, 2, 3]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

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

        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(account_state_pda),
            create_system_program_account(),
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

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![1, 2, 3]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

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

        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(account_state_pda),
            create_system_program_account(),
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

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![1, 2, 3]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

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

        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(account_state_pda),
            create_system_program_account(),
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

        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (expected_account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

        // Use a different account key than expected
        let wrong_account_key = Pubkey::new_unique();

        let packet_data =
            create_gmp_packet_data(client_id, sender, system_program::ID, salt, vec![]);
        let packet_data_bytes = packet_data.try_to_vec().unwrap();

        let recv_msg = create_recv_packet_msg(client_id, packet_data_bytes, 1);
        let instruction = create_recv_packet_instruction(
            ctx.app_state_pda,
            ctx.router_program,
            ctx.payer,
            recv_msg,
        );

        let accounts = vec![
            create_gmp_app_state_account(
                ctx.app_state_pda,
                ctx.router_program,
                ctx.authority,
                ctx.app_state_bump,
                false, // not paused
            ),
            create_router_program_account(ctx.router_program),
            create_authority_account(ctx.payer),
            create_system_program_account(),
            create_uninitialized_account_for_pda(wrong_account_key), // Wrong account key!
            create_system_program_account(),
        ];

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "OnRecvPacket should fail when account key doesn't match expected PDA (expected: {expected_account_state_pda}, got: {wrong_account_key})"
        );
    }

    #[test]
    fn test_on_recv_packet_success_with_cpi() {
        use gmp_counter_app::ID as COUNTER_APP_ID;
        use prost::Message as ProstMessage;
        use solana_sdk::account::Account;
        use solana_sdk::bpf_loader_upgradeable;

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
        let router_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        // Create packet data that will call the counter app
        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

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

        // Create SolanaInstruction that will increment the counter
        let counter_instruction = gmp_counter_app::instruction::Increment { amount: 5 };
        let counter_instruction_data = anchor_lang::InstructionData::data(&counter_instruction);

        // Build SolanaInstruction for the payload
        let solana_instruction = SolanaInstruction {
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
                SolanaAccountMeta {
                    pubkey: account_state_pda.to_bytes().to_vec(),
                    is_signer: true,
                    is_writable: false,
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

        let mut solana_instruction_bytes = Vec::new();
        solana_instruction
            .encode(&mut solana_instruction_bytes)
            .unwrap();

        // Create GMPPacketData with the counter instruction as payload using protobuf
        let proto_packet_data = crate::proto::GmpPacketData {
            sender: sender.to_string(),
            receiver: COUNTER_APP_ID.to_string(),
            salt: salt.clone(),
            payload: solana_instruction_bytes,
            memo: String::new(),
        };

        let mut packet_data_bytes = Vec::new();
        proto_packet_data.encode(&mut packet_data_bytes).unwrap();

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
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                // Remaining accounts for CPI:
                AccountMeta::new(account_state_pda, false), // [0] account_state (GMP account)
                AccountMeta::new_readonly(COUNTER_APP_ID, false), // [1] target_program
                AccountMeta::new(counter_app_state_pda, false), // [2] counter app state
                AccountMeta::new(user_counter_pda, false),  // [3] user counter
                AccountMeta::new_readonly(account_state_pda, false), // [4] user_authority (same as [0])
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
                router_program,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
            create_authority_account(payer),
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
            create_uninitialized_account_for_pda(user_counter_pda), // User counter will be created
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

        let ack = crate::state::GMPAcknowledgement::decode(ack_bytes).unwrap();

        assert!(
            ack.success,
            "CPI execution should succeed, but got error: {}",
            ack.error
        );

        // Verify account state was created and has correct data
        let acc = result
            .get_account(&account_state_pda)
            .expect("Account state should be created");

        assert_eq!(
            acc.owner,
            crate::ID,
            "Account should be owned by GMP program"
        );
        assert!(!acc.data.is_empty(), "Account should have data");

        // Check discriminator
        assert_eq!(
            &acc.data[0..crate::constants::DISCRIMINATOR_SIZE],
            AccountState::DISCRIMINATOR,
            "Should have correct discriminator"
        );

        // Deserialize and verify account state
        let account_state =
            AccountState::try_deserialize(&mut &acc.data[crate::constants::DISCRIMINATOR_SIZE..])
                .expect("Failed to deserialize account state");
        assert_eq!(account_state.nonce, 1, "Nonce should be incremented to 1");
        assert_eq!(account_state.client_id, client_id);
        assert_eq!(account_state.sender, sender);
        assert_eq!(account_state.salt, salt);
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
        use gmp_counter_app::ID as COUNTER_APP_ID;
        use prost::Message as ProstMessage;
        use solana_sdk::account::Account;
        use solana_sdk::bpf_loader_upgradeable;

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
        let client_id = "cosmoshub-1";
        let sender = "cosmos1test";
        let salt = vec![1u8, 2, 3];

        let (account_state_pda, _account_bump) =
            AccountState::derive_address(client_id, sender, &salt, &crate::ID).unwrap();

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

        // Build SolanaInstruction for the payload
        let solana_instruction = SolanaInstruction {
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
                SolanaAccountMeta {
                    pubkey: account_state_pda.to_bytes().to_vec(),
                    is_signer: true,
                    is_writable: false,
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

        let mut solana_instruction_bytes = Vec::new();
        solana_instruction
            .encode(&mut solana_instruction_bytes)
            .unwrap();

        // Create GMPPacketData with the counter instruction as payload using protobuf
        let proto_packet_data = crate::proto::GmpPacketData {
            sender: sender.to_string(),
            receiver: COUNTER_APP_ID.to_string(),
            salt,
            payload: solana_instruction_bytes,
            memo: String::new(),
        };

        let mut packet_data_bytes = Vec::new();
        proto_packet_data.encode(&mut packet_data_bytes).unwrap();

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
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                // Remaining accounts for CPI:
                AccountMeta::new(account_state_pda, false), // [0] account_state (GMP account)
                AccountMeta::new_readonly(COUNTER_APP_ID, false), // [1] target_program
                AccountMeta::new(counter_app_state_pda, false), // [2] counter app state
                AccountMeta::new(user_counter_pda, false),  // [3] user counter
                AccountMeta::new_readonly(account_state_pda, false), // [4] user_authority (same as [0])
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
                router_program,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_router_program_account(router_program),
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

        // Verify account state was NOT created (transaction rolled back)
        if let Some(acc) = result.get_account(&account_state_pda) {
            assert!(
                acc.data.is_empty() || acc.data.iter().all(|&b| b == 0),
                "Account should remain uninitialized after transaction abort"
            );
        }
    }
}
