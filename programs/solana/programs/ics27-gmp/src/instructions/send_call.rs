use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPCallSent;
use crate::state::{GMPAppState, SendCallMsg};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions as sysvar_instructions;
use solana_ibc_proto::{Protobuf, RawGmpPacketData};
use solana_ibc_types::{GmpPacketData, MsgSendPacket, Payload};

/// Send a GMP call packet
#[derive(Accounts)]
#[instruction(msg: SendCallMsg)]
pub struct SendCall<'info> {
    /// App state account - validated by Anchor PDA constraints
    /// This account will be signed when calling the router to prove GMP is the caller
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump,
        constraint = !app_state.paused @ GMPError::AppPaused
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Only used for direct calls (must sign). For CPI calls, this account is ignored
    /// and the calling program ID is extracted from instruction sysvar instead.
    /// CHECK: `UncheckedAccount` because validation depends on runtime call type.
    pub sender: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    /// Router program for sending packets
    pub router_program: Program<'info, ics26_router::program::Ics26Router>,

    /// Router state account
    /// CHECK: Router program validates this
    #[account()]
    pub router_state: AccountInfo<'info>,

    /// Client sequence account for packet sequencing
    /// CHECK: Router program validates this
    #[account(mut)]
    pub client_sequence: AccountInfo<'info>,

    /// Packet commitment account to be created
    /// CHECK: Router program validates this
    #[account(mut)]
    pub packet_commitment: AccountInfo<'info>,

    /// Instructions sysvar for detecting CPI vs direct call
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    /// IBC app registration account
    /// CHECK: Router program validates this
    #[account()]
    pub ibc_app: AccountInfo<'info>,

    /// Client account
    /// CHECK: Router program validates this
    #[account()]
    pub client: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

pub fn send_call(ctx: Context<SendCall>, msg: SendCallMsg) -> Result<u64> {
    let clock = Clock::get()?;
    let current_time = clock.unix_timestamp;

    // Direct call: sender signs. CPI call: use calling program ID for callback routing.
    solana_ibc_types::reject_nested_cpi().map_err(GMPError::from)?;
    let sender_pubkey = if solana_ibc_types::is_cpi() {
        let instruction_sysvar = ctx.accounts.instruction_sysvar.to_account_info();
        sysvar_instructions::get_instruction_relative(0, &instruction_sysvar)
            .map_err(|_| GMPError::InvalidSysvar)?
            .program_id
    } else {
        require!(ctx.accounts.sender.is_signer, GMPError::SenderMustSign);
        ctx.accounts.sender.key()
    };

    // Validate IBC routing fields
    let source_client = solana_ibc_types::ClientId::new(&msg.source_client)
        .map_err(|_| GMPError::InvalidClientId)?;

    // Validate timeout bounds
    require!(
        msg.timeout_timestamp > current_time + MIN_TIMEOUT_DURATION,
        GMPError::TimeoutTooSoon
    );
    require!(
        msg.timeout_timestamp < current_time + MAX_TIMEOUT_DURATION,
        GMPError::TimeoutTooLong
    );

    // Create raw GMP packet - sender is used for callback routing on timeout/ack
    let raw_packet_data = RawGmpPacketData {
        sender: sender_pubkey.to_string(),
        receiver: msg.receiver.clone(),
        salt: msg.salt.clone(),
        payload: msg.payload.clone(),
        memo: msg.memo.clone(),
    };

    // Validate GMP packet
    let packet_data = GmpPacketData::try_from(raw_packet_data).map_err(|e| {
        msg!("GMP packet validation failed: {}", e);
        GMPError::InvalidPacketData
    })?;

    // Encode to protobuf bytes
    let packet_data_bytes = packet_data.encode_vec();

    // Create IBC packet payload
    let ibc_payload = Payload {
        source_port: GMP_PORT_ID.to_string(),
        dest_port: GMP_PORT_ID.to_string(),
        version: ICS27_VERSION.to_string(),
        encoding: ICS27_ENCODING.to_string(),
        value: packet_data_bytes,
    };

    // Create send packet message for router
    let router_msg = MsgSendPacket {
        source_client: source_client.to_string(),
        timeout_timestamp: msg.timeout_timestamp,
        payload: ibc_payload,
    };

    // Get signer seeds for the app_state PDA to prove GMP is the caller
    let app_state = &ctx.accounts.app_state;
    let signer_seeds: &[&[u8]] = &[GMPAppState::SEED, GMP_PORT_ID.as_bytes(), &[app_state.bump]];

    // Call router via CPI to actually send the packet
    // GMP signs its app_state PDA to cryptographically prove it's the caller
    let sequence = crate::router_cpi::send_packet_cpi(
        &ctx.accounts.router_program,
        &ctx.accounts.router_state,
        &ctx.accounts.client_sequence,
        &ctx.accounts.packet_commitment,
        &ctx.accounts.app_state.to_account_info(),
        signer_seeds,
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.ibc_app,
        &ctx.accounts.client,
        &ctx.accounts.system_program.to_account_info(),
        router_msg,
    )?;

    // Emit event
    emit!(GMPCallSent {
        sequence,
        sender: sender_pubkey,
        receiver: msg.receiver.clone(),
        client_id: source_client.to_string(),
        salt: msg.salt.clone(),
        payload_size: msg.payload.len() as u64,
        timeout_timestamp: msg.timeout_timestamp,
    });

    msg!(
        "GMP call sent: sender={}, receiver={}, sequence={}",
        sender_pubkey,
        &msg.receiver,
        sequence
    );

    Ok(sequence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GMPAppState;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use rstest::rstest;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        system_program,
    };

    struct TestContext {
        mollusk: Mollusk,
        sender: Pubkey,
        payer: Pubkey,
        router_program: Pubkey,
        router_state: Pubkey,
        client_sequence: Pubkey,
        packet_commitment: Pubkey,
        ibc_app: Pubkey,
        client: Pubkey,
        app_state_pda: Pubkey,
        app_state_bump: u8,
    }

    impl TestContext {
        fn new() -> Self {
            let sender = Pubkey::new_unique();
            let payer = Pubkey::new_unique();
            let router_program = ics26_router::ID;
            let router_state = Pubkey::new_unique();
            let client_sequence = Pubkey::new_unique();
            let packet_commitment = Pubkey::new_unique();
            let ibc_app = Pubkey::new_unique();
            let client = Pubkey::new_unique();
            let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
                &[GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
                &crate::ID,
            );

            Self {
                mollusk: Mollusk::new(&crate::ID, crate::get_gmp_program_path()),
                sender,
                payer,
                router_program,
                router_state,
                client_sequence,
                packet_commitment,
                ibc_app,
                client,
                app_state_pda,
                app_state_bump,
            }
        }

        fn create_valid_msg() -> SendCallMsg {
            SendCallMsg {
                source_client: "cosmoshub-1".to_string(),
                receiver: Pubkey::new_unique().to_string(),
                salt: vec![1, 2, 3],
                payload: vec![4, 5, 6],
                timeout_timestamp: 3600, // 1 hour from epoch (safe for Mollusk default clock=0)
                memo: String::new(),
            }
        }

        fn build_instruction(&self, msg: SendCallMsg, sender_is_signer: bool) -> Instruction {
            let instruction_data = crate::instruction::SendCall { msg };

            Instruction {
                program_id: crate::ID,
                accounts: vec![
                    AccountMeta::new(self.app_state_pda, false),
                    AccountMeta::new_readonly(self.sender, sender_is_signer),
                    AccountMeta::new(self.payer, true),
                    AccountMeta::new_readonly(self.router_program, false),
                    AccountMeta::new_readonly(self.router_state, false),
                    AccountMeta::new(self.client_sequence, false),
                    AccountMeta::new(self.packet_commitment, false),
                    AccountMeta::new_readonly(
                        anchor_lang::solana_program::sysvar::instructions::ID,
                        false,
                    ),
                    AccountMeta::new_readonly(self.ibc_app, false),
                    AccountMeta::new_readonly(self.client, false),
                    AccountMeta::new_readonly(system_program::ID, false),
                ],
                data: instruction_data.data(),
            }
        }

        fn build_accounts(&self, paused: bool) -> Vec<(Pubkey, solana_sdk::account::Account)> {
            vec![
                create_gmp_app_state_account(self.app_state_pda, self.app_state_bump, paused),
                create_authority_account(self.sender),
                create_authority_account(self.payer),
                create_router_program_account(self.router_program),
                create_authority_account(self.router_state),
                create_authority_account(self.client_sequence),
                create_authority_account(self.packet_commitment),
                create_instructions_sysvar_account(),
                create_authority_account(self.ibc_app),
                create_authority_account(self.client),
                create_system_program_account(),
            ]
        }

        fn build_instruction_with_wrong_pda(
            &self,
            msg: SendCallMsg,
            wrong_pda: Pubkey,
        ) -> Instruction {
            let instruction_data = crate::instruction::SendCall { msg };

            Instruction {
                program_id: crate::ID,
                accounts: vec![
                    AccountMeta::new(wrong_pda, false), // Wrong PDA!
                    AccountMeta::new_readonly(self.sender, true),
                    AccountMeta::new(self.payer, true),
                    AccountMeta::new_readonly(self.router_program, false),
                    AccountMeta::new_readonly(self.router_state, false),
                    AccountMeta::new(self.client_sequence, false),
                    AccountMeta::new(self.packet_commitment, false),
                    AccountMeta::new_readonly(
                        anchor_lang::solana_program::sysvar::instructions::ID,
                        false,
                    ),
                    AccountMeta::new_readonly(self.ibc_app, false),
                    AccountMeta::new_readonly(self.client, false),
                    AccountMeta::new_readonly(system_program::ID, false),
                ],
                data: instruction_data.data(),
            }
        }

        fn build_accounts_with_wrong_pda(
            &self,
            wrong_pda: Pubkey,
        ) -> Vec<(Pubkey, solana_sdk::account::Account)> {
            vec![
                create_gmp_app_state_account(wrong_pda, self.app_state_bump, false),
                create_authority_account(self.sender),
                create_authority_account(self.payer),
                create_router_program_account(self.router_program),
                create_authority_account(self.router_state),
                create_authority_account(self.client_sequence),
                create_authority_account(self.packet_commitment),
                create_instructions_sysvar_account(),
                create_authority_account(self.ibc_app),
                create_authority_account(self.client),
                create_system_program_account(),
            ]
        }

        fn build_instruction_with_wrong_router(
            &self,
            msg: SendCallMsg,
            wrong_router: Pubkey,
        ) -> Instruction {
            let instruction_data = crate::instruction::SendCall { msg };

            Instruction {
                program_id: crate::ID,
                accounts: vec![
                    AccountMeta::new(self.app_state_pda, false),
                    AccountMeta::new_readonly(self.sender, true),
                    AccountMeta::new(self.payer, true),
                    AccountMeta::new_readonly(wrong_router, false), // Wrong router!
                    AccountMeta::new_readonly(self.router_state, false),
                    AccountMeta::new(self.client_sequence, false),
                    AccountMeta::new(self.packet_commitment, false),
                    AccountMeta::new_readonly(
                        anchor_lang::solana_program::sysvar::instructions::ID,
                        false,
                    ),
                    AccountMeta::new_readonly(self.ibc_app, false),
                    AccountMeta::new_readonly(self.client, false),
                    AccountMeta::new_readonly(system_program::ID, false),
                ],
                data: instruction_data.data(),
            }
        }

        fn build_accounts_with_wrong_router(
            &self,
            wrong_router: Pubkey,
        ) -> Vec<(Pubkey, solana_sdk::account::Account)> {
            vec![
                create_gmp_app_state_account(self.app_state_pda, self.app_state_bump, false),
                create_authority_account(self.sender),
                create_authority_account(self.payer),
                create_router_program_account(wrong_router),
                create_authority_account(self.router_state),
                create_authority_account(self.client_sequence),
                create_authority_account(self.packet_commitment),
                create_instructions_sysvar_account(),
                create_authority_account(self.ibc_app),
                create_authority_account(self.client),
                create_system_program_account(),
            ]
        }
    }

    #[derive(Clone, Copy)]
    enum SendCallErrorCase {
        AppPaused,
        SenderNotSigner,
        InvalidAppStatePda,
        WrongRouterProgram,
        EmptyPayload,
        SaltTooLong,
        MemoTooLong,
        ReceiverTooLong,
        TimeoutTooSoon,
        TimeoutTooLong,
        EmptyClientId,
    }

    fn run_send_call_error_test(case: SendCallErrorCase) {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();

        let (instruction, accounts) = match case {
            SendCallErrorCase::AppPaused => {
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(true); // paused
                (instruction, accounts)
            }
            SendCallErrorCase::SenderNotSigner => {
                let instruction = ctx.build_instruction(msg, false); // sender not signer
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
            SendCallErrorCase::InvalidAppStatePda => {
                let wrong_pda = Pubkey::new_unique();
                let instruction = ctx.build_instruction_with_wrong_pda(msg, wrong_pda);
                let accounts = ctx.build_accounts_with_wrong_pda(wrong_pda);
                (instruction, accounts)
            }
            SendCallErrorCase::WrongRouterProgram => {
                let wrong_router = Pubkey::new_unique();
                let instruction = ctx.build_instruction_with_wrong_router(msg, wrong_router);
                let accounts = ctx.build_accounts_with_wrong_router(wrong_router);
                (instruction, accounts)
            }
            SendCallErrorCase::EmptyPayload => {
                msg.payload = vec![];
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
            SendCallErrorCase::SaltTooLong => {
                msg.salt = vec![0u8; crate::constants::MAX_SALT_LENGTH + 1];
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
            SendCallErrorCase::MemoTooLong => {
                msg.memo = "x".repeat(crate::constants::MAX_MEMO_LENGTH + 1);
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
            SendCallErrorCase::ReceiverTooLong => {
                msg.receiver = "x".repeat(crate::constants::MAX_RECEIVER_LENGTH + 1);
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
            SendCallErrorCase::TimeoutTooSoon => {
                msg.timeout_timestamp = 1; // Too soon (less than MIN_TIMEOUT_DURATION from clock=0)
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
            SendCallErrorCase::TimeoutTooLong => {
                msg.timeout_timestamp = i64::MAX;
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
            SendCallErrorCase::EmptyClientId => {
                msg.source_client = String::new();
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts)
            }
        };

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[rstest]
    #[case::app_paused(SendCallErrorCase::AppPaused)]
    #[case::sender_not_signer(SendCallErrorCase::SenderNotSigner)]
    #[case::invalid_app_state_pda(SendCallErrorCase::InvalidAppStatePda)]
    #[case::wrong_router_program(SendCallErrorCase::WrongRouterProgram)]
    #[case::empty_payload(SendCallErrorCase::EmptyPayload)]
    #[case::salt_too_long(SendCallErrorCase::SaltTooLong)]
    #[case::memo_too_long(SendCallErrorCase::MemoTooLong)]
    #[case::receiver_too_long(SendCallErrorCase::ReceiverTooLong)]
    #[case::timeout_too_soon(SendCallErrorCase::TimeoutTooSoon)]
    #[case::timeout_too_long(SendCallErrorCase::TimeoutTooLong)]
    #[case::empty_client_id(SendCallErrorCase::EmptyClientId)]
    fn test_send_call_validation(#[case] case: SendCallErrorCase) {
        run_send_call_error_test(case);
    }
}

/// Integration tests using ProgramTest with real BPF runtime.
///
/// These test the CPI detection logic (lines 72-82 of send_call) which depends
/// on `get_stack_height()` — a syscall that returns 0 in Mollusk but works
/// correctly under ProgramTest's BPF execution.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::state::GMPAppState;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signer::Signer,
    };

    const SENDER_MUST_SIGN_ERROR: u32 =
        anchor_lang::error::ERROR_CODE_OFFSET + GMPError::SenderMustSign as u32;

    fn build_send_call_ix(payer: Pubkey, sender: Pubkey, sender_is_signer: bool) -> Instruction {
        let (app_state_pda, _) = Pubkey::find_program_address(
            &[GMPAppState::SEED, crate::constants::GMP_PORT_ID.as_bytes()],
            &crate::ID,
        );

        let msg = SendCallMsg {
            source_client: "cosmoshub-1".to_string(),
            receiver: Pubkey::new_unique().to_string(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            timeout_timestamp: 3600,
            memo: String::new(),
        };

        let ix_data = crate::instruction::SendCall { msg };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, sender_is_signer),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(ics26_router::ID, false),
                AccountMeta::new_readonly(Pubkey::new_unique(), false), // router_state
                AccountMeta::new(Pubkey::new_unique(), false),          // client_sequence
                AccountMeta::new(Pubkey::new_unique(), false),          // packet_commitment
                AccountMeta::new_readonly(
                    anchor_lang::solana_program::sysvar::instructions::ID,
                    false,
                ),
                AccountMeta::new_readonly(Pubkey::new_unique(), false), // ibc_app
                AccountMeta::new_readonly(Pubkey::new_unique(), false), // client
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: ix_data.data(),
        }
    }

    /// Direct call with sender not signing must fail with SenderMustSign.
    /// This proves the direct-call path (is_cpi() == false) is taken.
    #[tokio::test]
    async fn test_direct_call_requires_sender_signer() {
        let pt = setup_program_test();
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let sender = Pubkey::new_unique();
        let ix = build_send_call_ix(payer.pubkey(), sender, false);

        let result = process_tx(&banks_client, &payer, recent_blockhash, &[ix]).await;
        let err = result.expect_err("direct call without signer should fail");
        assert_eq!(
            extract_custom_error(&err),
            Some(SENDER_MUST_SIGN_ERROR),
            "expected SenderMustSign (6044), got: {err:?}"
        );
    }

    /// CPI call (Tx -> malicious_caller -> GMP) with sender not signing should
    /// NOT fail with SenderMustSign — proving is_cpi() returns true and the
    /// signer check is bypassed. It will fail later at the router CPI (router
    /// is not initialized) but with a different error.
    #[tokio::test]
    async fn test_cpi_call_skips_sender_signer_check() {
        let pt = setup_program_test();
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let sender = Pubkey::new_unique();
        let inner_ix = build_send_call_ix(payer.pubkey(), sender, false);
        let ix = wrap_in_proxy_cpi(payer.pubkey(), &inner_ix);

        let result = process_tx(&banks_client, &payer, recent_blockhash, &[ix]).await;
        let err = result.expect_err("CPI call should still fail (router not initialized)");
        assert_ne!(
            extract_custom_error(&err),
            Some(SENDER_MUST_SIGN_ERROR),
            "CPI call must NOT fail with SenderMustSign — is_cpi() should be true"
        );
    }

    /// Nested CPI (Tx -> malicious_caller -> cpi_test_target -> GMP)
    /// must be rejected with UnauthorizedRouter (mapped from NestedCpiNotAllowed).
    #[tokio::test]
    async fn test_nested_cpi_rejected() {
        let pt = setup_program_test();
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let sender = Pubkey::new_unique();
        let inner_ix = build_send_call_ix(payer.pubkey(), sender, false);
        let middle_ix = wrap_in_cpi_test_target_proxy(payer.pubkey(), &inner_ix);
        let ix = wrap_in_proxy_cpi(payer.pubkey(), &middle_ix);

        let result = process_tx(&banks_client, &payer, recent_blockhash, &[ix]).await;
        let err = result.expect_err("nested CPI should be rejected");
        assert_eq!(
            extract_custom_error(&err),
            Some(
                anchor_lang::error::ERROR_CODE_OFFSET
                    + crate::errors::GMPError::UnauthorizedRouter as u32
            ),
            "expected UnauthorizedRouter (from NestedCpiNotAllowed), got: {err:?}"
        );
    }
}
