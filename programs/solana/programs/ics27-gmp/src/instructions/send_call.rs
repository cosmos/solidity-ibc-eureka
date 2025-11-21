use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPCallSent;
use crate::state::{GMPAppState, SendCallMsg};
use anchor_lang::prelude::*;
use solana_ibc_types::{MsgSendPacket, Payload, ValidatedGmpPacketData};

/// Send a GMP call packet
#[derive(Accounts)]
#[instruction(msg: SendCallMsg)]
pub struct SendCall<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump,
        constraint = !app_state.paused @ GMPError::AppPaused
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Sender of the call
    pub sender: Signer<'info>,

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

    /// Instructions sysvar for router CPI validation
    /// CHECK: Router program validates this
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

    // Create, validate, and encode packet data
    let validated_packet_data = ValidatedGmpPacketData::new(
        ctx.accounts.sender.key().to_string(),
        msg.receiver.clone(),
        msg.salt.clone(),
        msg.payload.clone(),
        msg.memo.clone(),
    )
    .map_err(GMPError::from)?;

    let packet_data_bytes = validated_packet_data.encode_to_vec();

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

    // Call router via CPI to actually send the packet
    let sequence = crate::router_cpi::send_packet_cpi(
        &ctx.accounts.router_program,
        &ctx.accounts.router_state,
        &ctx.accounts.client_sequence,
        &ctx.accounts.packet_commitment,
        &ctx.accounts.instruction_sysvar,
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.ibc_app,
        &ctx.accounts.client,
        &ctx.accounts.system_program.to_account_info(),
        router_msg,
    )?;

    // Emit event
    emit!(GMPCallSent {
        sequence,
        sender: ctx.accounts.sender.key(),
        receiver: msg.receiver.clone(),
        client_id: source_client.to_string(),
        salt: msg.salt.clone(),
        payload_size: msg.payload.len() as u64,
        timeout_timestamp: msg.timeout_timestamp,
    });

    msg!(
        "GMP call sent: sender={}, receiver={}, sequence={}",
        ctx.accounts.sender.key(),
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
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        system_program,
    };

    struct TestContext {
        mollusk: Mollusk,
        authority: Pubkey,
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
            let authority = Pubkey::new_unique();
            let sender = Pubkey::new_unique();
            let payer = Pubkey::new_unique();
            let router_program = Pubkey::new_unique();
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
                authority,
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

        fn build_instruction(&self, msg: SendCallMsg) -> Instruction {
            let instruction_data = crate::instruction::SendCall { msg };

            Instruction {
                program_id: crate::ID,
                accounts: vec![
                    AccountMeta::new(self.app_state_pda, false),
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

        fn build_accounts(&self, paused: bool) -> Vec<(Pubkey, solana_sdk::account::Account)> {
            vec![
                create_gmp_app_state_account(
                    self.app_state_pda,
                    self.authority,
                    self.app_state_bump,
                    paused,
                ),
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

        fn create_valid_msg() -> SendCallMsg {
            SendCallMsg {
                source_client: "cosmoshub-1".to_string(),
                receiver: Pubkey::new_unique().to_string(),
                salt: vec![1, 2, 3],
                payload: vec![4, 5, 6],
                timeout_timestamp: 9_999_999_999,
                memo: String::new(),
            }
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
                create_gmp_app_state_account(wrong_pda, self.authority, self.app_state_bump, false),
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
                create_gmp_app_state_account(
                    self.app_state_pda,
                    self.authority,
                    self.app_state_bump,
                    false,
                ),
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

    #[test]
    fn test_send_call_app_paused() {
        let ctx = TestContext::new();
        let msg = TestContext::create_valid_msg();
        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(true); // paused

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail when app is paused"
        );
    }

    #[test]
    fn test_send_call_invalid_timeout() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.timeout_timestamp = 1_000_000; // Timeout in the past

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with timeout in the past"
        );
    }

    #[test]
    fn test_send_call_invalid_app_state_pda() {
        let ctx = TestContext::new();
        let msg = TestContext::create_valid_msg();
        let wrong_pda = Pubkey::new_unique();
        let instruction = ctx.build_instruction_with_wrong_pda(msg, wrong_pda);
        let accounts = ctx.build_accounts_with_wrong_pda(wrong_pda);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with invalid app_state PDA"
        );
    }

    #[test]
    fn test_send_call_wrong_router_program() {
        let ctx = TestContext::new();
        let msg = TestContext::create_valid_msg();
        let wrong_router = Pubkey::new_unique();
        let instruction = ctx.build_instruction_with_wrong_router(msg, wrong_router);
        let accounts = ctx.build_accounts_with_wrong_router(wrong_router);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with wrong router_program"
        );
    }

    #[test]
    fn test_send_call_empty_payload() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.payload = vec![];

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with empty payload"
        );
    }

    #[test]
    fn test_send_call_salt_too_long() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.salt = vec![0u8; crate::constants::MAX_SALT_LENGTH + 1];

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with salt too long"
        );
    }

    #[test]
    fn test_send_call_memo_too_long() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.memo = "x".repeat(crate::constants::MAX_MEMO_LENGTH + 1);

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with memo too long"
        );
    }

    #[test]
    fn test_send_call_receiver_too_long() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.receiver = "x".repeat(crate::constants::MAX_RECEIVER_LENGTH + 1);

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with receiver too long"
        );
    }

    #[test]
    fn test_send_call_timeout_too_far_in_future() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.timeout_timestamp = i64::MAX;

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with timeout too far in the future"
        );
    }

    #[test]
    fn test_send_call_receiver_empty() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.receiver = String::new();

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "SendCall should succeed with empty receiver (for native cosmos modules)"
        );
    }

    #[test]
    fn test_send_call_empty_client_id() {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();
        msg.source_client = String::new();

        let instruction = ctx.build_instruction(msg);
        let accounts = ctx.build_accounts(false);

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with empty client_id"
        );
    }
}
