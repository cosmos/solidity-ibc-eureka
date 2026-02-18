use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPCallSent;
use crate::state::{GMPAppState, SendCallMsg};
use anchor_lang::prelude::*;
use ics26_router::state::{Client, ClientSequence, IBCApp, RouterState};
use solana_ibc_proto::{Protobuf, RawGmpPacketData};
use solana_ibc_types::{GmpPacketData, MsgSendPacket, Payload};

/// Send a GMP call packet (direct wallet call only, rejects CPI)
#[derive(Accounts)]
#[instruction(msg: SendCallMsg)]
pub struct SendCall<'info> {
    #[account(
        mut,
        seeds = [GMPAppState::SEED],
        bump = app_state.bump,
        constraint = !app_state.paused @ GMPError::AppPaused
    )]
    pub app_state: Account<'info, GMPAppState>,

    pub sender: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub router_program: Program<'info, ics26_router::program::Ics26Router>,

    #[account(
        seeds = [RouterState::SEED],
        bump,
        seeds::program = router_program
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        mut,
        seeds = [ClientSequence::SEED, msg.source_client.as_bytes()],
        bump,
        seeds::program = router_program
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    /// CHECK: PDA validated by router (sequence computed at runtime)
    #[account(mut)]
    pub packet_commitment: AccountInfo<'info>,

    #[account(
        seeds = [IBCApp::SEED, GMP_PORT_ID.as_bytes()],
        bump,
        seeds::program = router_program
    )]
    pub ibc_app: Account<'info, IBCApp>,

    #[account(
        seeds = [Client::SEED, msg.source_client.as_bytes()],
        bump,
        seeds::program = router_program
    )]
    pub client: Account<'info, Client>,

    /// CHECK: Validated against client registry
    #[account(address = client.client_program_id @ GMPError::InvalidLightClientProgram)]
    pub light_client_program: AccountInfo<'info>,

    /// CHECK: Ownership validated against light client program
    #[account(owner = light_client_program.key() @ GMPError::InvalidAccountOwner)]
    pub client_state: AccountInfo<'info>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

pub fn send_call(ctx: Context<SendCall>, msg: SendCallMsg) -> Result<u64> {
    solana_ibc_types::reject_cpi(&ctx.accounts.instruction_sysvar, &crate::ID)
        .map_err(GMPError::from)?;

    let sender_pubkey = ctx.accounts.sender.key();

    send_call_inner(
        &ctx.accounts.app_state,
        &ctx.accounts.router_program,
        &ctx.accounts.router_state,
        &ctx.accounts.client_sequence,
        &ctx.accounts.packet_commitment,
        &ctx.accounts.payer,
        &ctx.accounts.ibc_app,
        &ctx.accounts.client,
        &ctx.accounts.light_client_program,
        &ctx.accounts.client_state,
        &ctx.accounts.system_program,
        sender_pubkey,
        msg,
    )
}

/// Shared logic for `send_call` and `send_call_cpi`
#[allow(clippy::too_many_arguments)]
pub(crate) fn send_call_inner<'info>(
    app_state: &Account<'info, GMPAppState>,
    router_program: &Program<'info, ics26_router::program::Ics26Router>,
    router_state: &Account<'info, RouterState>,
    client_sequence: &Account<'info, ClientSequence>,
    packet_commitment: &AccountInfo<'info>,
    payer: &Signer<'info>,
    ibc_app: &Account<'info, IBCApp>,
    client: &Account<'info, Client>,
    light_client_program: &AccountInfo<'info>,
    client_state: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    sender_pubkey: Pubkey,
    msg: SendCallMsg,
) -> Result<u64> {
    let clock = Clock::get()?;
    let current_time = clock.unix_timestamp;

    let source_client = solana_ibc_types::ClientId::new(&msg.source_client)
        .map_err(|_| GMPError::InvalidClientId)?;

    require!(
        msg.timeout_timestamp > current_time + MIN_TIMEOUT_DURATION,
        GMPError::TimeoutTooSoon
    );
    require!(
        msg.timeout_timestamp < current_time + MAX_TIMEOUT_DURATION,
        GMPError::TimeoutTooLong
    );

    let raw_packet_data = RawGmpPacketData {
        sender: sender_pubkey.to_string(),
        receiver: msg.receiver.clone(),
        salt: msg.salt.clone(),
        payload: msg.payload.clone(),
        memo: msg.memo.clone(),
    };

    let packet_data = GmpPacketData::try_from(raw_packet_data).map_err(|e| {
        msg!("GMP packet validation failed: {}", e);
        GMPError::InvalidPacketData
    })?;

    let packet_data_bytes = packet_data.encode_vec();

    let ibc_payload = Payload {
        source_port: GMP_PORT_ID.to_string(),
        dest_port: GMP_PORT_ID.to_string(),
        version: ICS27_VERSION.to_string(),
        encoding: ICS27_ENCODING.to_string(),
        value: packet_data_bytes,
    };

    let router_msg = MsgSendPacket {
        source_client: source_client.to_string(),
        timeout_timestamp: msg.timeout_timestamp,
        payload: ibc_payload,
    };

    let sequence = crate::router_cpi::send_packet_cpi(
        router_program,
        &router_state.to_account_info(),
        &client_sequence.to_account_info(),
        packet_commitment,
        &app_state.to_account_info(),
        app_state.bump,
        &payer.to_account_info(),
        &ibc_app.to_account_info(),
        &client.to_account_info(),
        light_client_program,
        client_state,
        &system_program.to_account_info(),
        router_msg,
    )?;

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
        light_client_program: Pubkey,
        client_state: Pubkey,
        app_state_pda: Pubkey,
        app_state_bump: u8,
    }

    impl TestContext {
        fn new() -> Self {
            let sender = Pubkey::new_unique();
            let payer = Pubkey::new_unique();
            let router_program = ics26_router::ID;
            let (router_state, _) = create_router_state_pda();
            let (client_sequence, _) = create_client_sequence_pda(TEST_SOURCE_CLIENT);
            let packet_commitment = Pubkey::new_unique();
            let (ibc_app, _) = create_ibc_app_pda(GMP_PORT_ID);
            let light_client_program = Pubkey::new_unique();
            let (client, _) = create_client_pda(TEST_SOURCE_CLIENT, light_client_program);
            let client_state = Pubkey::new_unique();
            let (app_state_pda, app_state_bump) =
                Pubkey::find_program_address(&[GMPAppState::SEED], &crate::ID);

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
                light_client_program,
                client_state,
                app_state_pda,
                app_state_bump,
            }
        }

        fn create_valid_msg() -> SendCallMsg {
            SendCallMsg {
                source_client: TEST_SOURCE_CLIENT.to_string(),
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
                    AccountMeta::new_readonly(self.ibc_app, false),
                    AccountMeta::new_readonly(self.client, false),
                    AccountMeta::new_readonly(self.light_client_program, false),
                    AccountMeta::new_readonly(self.client_state, false),
                    AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
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
                create_router_state_pda(),
                create_client_sequence_pda(TEST_SOURCE_CLIENT),
                create_authority_account(self.packet_commitment),
                create_ibc_app_pda(GMP_PORT_ID),
                create_client_pda(TEST_SOURCE_CLIENT, self.light_client_program),
                create_authority_account(self.light_client_program),
                create_owned_account(self.client_state, self.light_client_program),
                create_instructions_sysvar_account_with_caller(crate::ID),
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
                    AccountMeta::new_readonly(self.ibc_app, false),
                    AccountMeta::new_readonly(self.client, false),
                    AccountMeta::new_readonly(self.light_client_program, false),
                    AccountMeta::new_readonly(self.client_state, false),
                    AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
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
                create_router_state_pda(),
                create_client_sequence_pda(TEST_SOURCE_CLIENT),
                create_authority_account(self.packet_commitment),
                create_ibc_app_pda(GMP_PORT_ID),
                create_client_pda(TEST_SOURCE_CLIENT, self.light_client_program),
                create_authority_account(self.light_client_program),
                create_owned_account(self.client_state, self.light_client_program),
                create_instructions_sysvar_account_with_caller(crate::ID),
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
                    AccountMeta::new_readonly(self.ibc_app, false),
                    AccountMeta::new_readonly(self.client, false),
                    AccountMeta::new_readonly(self.light_client_program, false),
                    AccountMeta::new_readonly(self.client_state, false),
                    AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
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
                create_router_state_pda(),
                create_client_sequence_pda(TEST_SOURCE_CLIENT),
                create_authority_account(self.packet_commitment),
                create_ibc_app_pda(GMP_PORT_ID),
                create_client_pda(TEST_SOURCE_CLIENT, self.light_client_program),
                create_authority_account(self.light_client_program),
                create_owned_account(self.client_state, self.light_client_program),
                create_instructions_sysvar_account_with_caller(crate::ID),
                create_system_program_account(),
            ]
        }
    }

    const ANCHOR_CONSTRAINT_SEEDS: u32 = 2006;
    const ANCHOR_INVALID_PROGRAM_ID: u32 = 3008;

    fn gmp_error(err: GMPError) -> u32 {
        anchor_lang::error::ERROR_CODE_OFFSET + err as u32
    }

    #[derive(Clone, Copy)]
    enum SendCallErrorCase {
        AppPaused,
        SenderNotSigner,
        FakeSysvar,
        InvalidAppStatePda,
        WrongRouterProgram,
        WrongRouterStatePda,
        WrongClientSequencePda,
        WrongIbcAppPda,
        WrongClientPda,
        WrongLightClientProgram,
        WrongClientStateOwner,
        EmptyPayload,
        SaltTooLong,
        MemoTooLong,
        ReceiverTooLong,
        TimeoutTooSoon,
        TimeoutTooLong,
        EmptyClientId,
    }

    const ANCHOR_SIGNER_ERROR: u32 = anchor_lang::error::ErrorCode::AccountNotSigner as u32;

    fn run_send_call_error_test(case: SendCallErrorCase) {
        let ctx = TestContext::new();
        let mut msg = TestContext::create_valid_msg();

        let (instruction, accounts, expected_error) = match case {
            SendCallErrorCase::AppPaused => {
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(true);
                (instruction, accounts, gmp_error(GMPError::AppPaused))
            }
            SendCallErrorCase::SenderNotSigner => {
                let instruction = ctx.build_instruction(msg, false);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts, ANCHOR_SIGNER_ERROR)
            }
            SendCallErrorCase::FakeSysvar => {
                let fake_sysvar = create_fake_instructions_sysvar_account(Pubkey::new_unique());
                let mut instruction = ctx.build_instruction(msg, true);
                let mut accounts = ctx.build_accounts(false);
                instruction.accounts[11] = AccountMeta::new_readonly(fake_sysvar.0, false);
                accounts[11] = fake_sysvar;
                (
                    instruction,
                    accounts,
                    anchor_lang::error::ErrorCode::ConstraintAddress as u32,
                )
            }
            SendCallErrorCase::InvalidAppStatePda => {
                let wrong_pda = Pubkey::new_unique();
                let instruction = ctx.build_instruction_with_wrong_pda(msg, wrong_pda);
                let accounts = ctx.build_accounts_with_wrong_pda(wrong_pda);
                (instruction, accounts, ANCHOR_CONSTRAINT_SEEDS)
            }
            SendCallErrorCase::WrongRouterProgram => {
                let wrong_router = Pubkey::new_unique();
                let instruction = ctx.build_instruction_with_wrong_router(msg, wrong_router);
                let accounts = ctx.build_accounts_with_wrong_router(wrong_router);
                (instruction, accounts, ANCHOR_INVALID_PROGRAM_ID)
            }
            SendCallErrorCase::WrongRouterStatePda => {
                let wrong_pda = Pubkey::new_unique();
                let mut instruction = ctx.build_instruction(msg, true);
                let mut accounts = ctx.build_accounts(false);
                instruction.accounts[4] = AccountMeta::new_readonly(wrong_pda, false);
                accounts[4].0 = wrong_pda;
                (instruction, accounts, ANCHOR_CONSTRAINT_SEEDS)
            }
            SendCallErrorCase::WrongClientSequencePda => {
                let wrong_pda = Pubkey::new_unique();
                let mut instruction = ctx.build_instruction(msg, true);
                let mut accounts = ctx.build_accounts(false);
                instruction.accounts[5] = AccountMeta::new(wrong_pda, false);
                accounts[5].0 = wrong_pda;
                (instruction, accounts, ANCHOR_CONSTRAINT_SEEDS)
            }
            SendCallErrorCase::WrongIbcAppPda => {
                let wrong_pda = Pubkey::new_unique();
                let mut instruction = ctx.build_instruction(msg, true);
                let mut accounts = ctx.build_accounts(false);
                instruction.accounts[7] = AccountMeta::new_readonly(wrong_pda, false);
                accounts[7].0 = wrong_pda;
                (instruction, accounts, ANCHOR_CONSTRAINT_SEEDS)
            }
            SendCallErrorCase::WrongClientPda => {
                let wrong_pda = Pubkey::new_unique();
                let mut instruction = ctx.build_instruction(msg, true);
                let mut accounts = ctx.build_accounts(false);
                instruction.accounts[8] = AccountMeta::new_readonly(wrong_pda, false);
                accounts[8].0 = wrong_pda;
                (instruction, accounts, ANCHOR_CONSTRAINT_SEEDS)
            }
            SendCallErrorCase::WrongLightClientProgram => {
                let wrong_program = Pubkey::new_unique();
                let mut instruction = ctx.build_instruction(msg, true);
                let mut accounts = ctx.build_accounts(false);
                instruction.accounts[9] = AccountMeta::new_readonly(wrong_program, false);
                accounts[9] = create_authority_account(wrong_program);
                (
                    instruction,
                    accounts,
                    gmp_error(GMPError::InvalidLightClientProgram),
                )
            }
            SendCallErrorCase::WrongClientStateOwner => {
                let wrong_owner = Pubkey::new_unique();
                let instruction = ctx.build_instruction(msg, true);
                let mut accounts = ctx.build_accounts(false);
                accounts[10] = create_owned_account(ctx.client_state, wrong_owner);
                (
                    instruction,
                    accounts,
                    gmp_error(GMPError::InvalidAccountOwner),
                )
            }
            SendCallErrorCase::EmptyPayload => {
                msg.payload = vec![];
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (
                    instruction,
                    accounts,
                    gmp_error(GMPError::InvalidPacketData),
                )
            }
            SendCallErrorCase::SaltTooLong => {
                msg.salt = vec![0u8; crate::constants::MAX_SALT_LENGTH + 1];
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (
                    instruction,
                    accounts,
                    gmp_error(GMPError::InvalidPacketData),
                )
            }
            SendCallErrorCase::MemoTooLong => {
                msg.memo = "x".repeat(crate::constants::MAX_MEMO_LENGTH + 1);
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (
                    instruction,
                    accounts,
                    gmp_error(GMPError::InvalidPacketData),
                )
            }
            SendCallErrorCase::ReceiverTooLong => {
                msg.receiver = "x".repeat(crate::constants::MAX_RECEIVER_LENGTH + 1);
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (
                    instruction,
                    accounts,
                    gmp_error(GMPError::InvalidPacketData),
                )
            }
            SendCallErrorCase::TimeoutTooSoon => {
                msg.timeout_timestamp = 1;
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts, gmp_error(GMPError::TimeoutTooSoon))
            }
            SendCallErrorCase::TimeoutTooLong => {
                msg.timeout_timestamp = i64::MAX;
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts, gmp_error(GMPError::TimeoutTooLong))
            }
            SendCallErrorCase::EmptyClientId => {
                msg.source_client = String::new();
                let instruction = ctx.build_instruction(msg, true);
                let accounts = ctx.build_accounts(false);
                (instruction, accounts, ANCHOR_CONSTRAINT_SEEDS)
            }
        };

        let result = ctx.mollusk.process_instruction(&instruction, &accounts);
        assert_eq!(
            result.program_result,
            Err(solana_sdk::instruction::InstructionError::Custom(
                expected_error
            ))
            .into(),
        );
    }

    #[rstest]
    #[case::app_paused(SendCallErrorCase::AppPaused)]
    #[case::sender_not_signer(SendCallErrorCase::SenderNotSigner)]
    #[case::fake_sysvar(SendCallErrorCase::FakeSysvar)]
    #[case::invalid_app_state_pda(SendCallErrorCase::InvalidAppStatePda)]
    #[case::wrong_router_program(SendCallErrorCase::WrongRouterProgram)]
    #[case::wrong_router_state_pda(SendCallErrorCase::WrongRouterStatePda)]
    #[case::wrong_client_sequence_pda(SendCallErrorCase::WrongClientSequencePda)]
    #[case::wrong_ibc_app_pda(SendCallErrorCase::WrongIbcAppPda)]
    #[case::wrong_client_pda(SendCallErrorCase::WrongClientPda)]
    #[case::wrong_light_client_program(SendCallErrorCase::WrongLightClientProgram)]
    #[case::wrong_client_state_owner(SendCallErrorCase::WrongClientStateOwner)]
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

/// Integration tests using `ProgramTest` with real BPF runtime.
///
/// These test the CPI detection logic (lines 72-82 of `send_call`) which depends
/// on `get_stack_height()` — a syscall that returns 0 in Mollusk but works
/// correctly under `ProgramTest`'s BPF execution.
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

    fn build_send_call_ix(payer: Pubkey, sender: Pubkey, sender_is_signer: bool) -> Instruction {
        let (app_state_pda, _) = Pubkey::find_program_address(&[GMPAppState::SEED], &crate::ID);

        let msg = SendCallMsg {
            source_client: TEST_SOURCE_CLIENT.to_string(),
            receiver: Pubkey::new_unique().to_string(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            timeout_timestamp: 3600,
            memo: String::new(),
        };

        let (router_state, _) = create_router_state_pda();
        let (client_sequence, _) = create_client_sequence_pda(TEST_SOURCE_CLIENT);
        let (ibc_app, _) = create_ibc_app_pda(crate::constants::GMP_PORT_ID);
        let (client, _) = create_client_pda(TEST_SOURCE_CLIENT, TEST_LIGHT_CLIENT_ID);

        let ix_data = crate::instruction::SendCall { msg };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, sender_is_signer),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(ics26_router::ID, false),
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(Pubkey::new_unique(), false), // packet_commitment
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(TEST_LIGHT_CLIENT_ID, false), // light_client_program
                AccountMeta::new_readonly(TEST_CLIENT_STATE_ID, false), // client_state
                AccountMeta::new_readonly(
                    anchor_lang::solana_program::sysvar::instructions::ID,
                    false,
                ),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: ix_data.data(),
        }
    }

    /// Direct call with sender not signing must fail with `AccountNotSigner`.
    /// Anchor enforces this via the `Signer<'info>` constraint on sender.
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
            Some(anchor_lang::error::ErrorCode::AccountNotSigner as u32),
            "expected AccountNotSigner, got: {err:?}"
        );
    }
}
