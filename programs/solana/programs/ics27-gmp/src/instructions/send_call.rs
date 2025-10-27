use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPCallSent;
use crate::proto::GmpPacketData;
use crate::state::{GMPAppState, SendCallMsg};
use anchor_lang::prelude::*;
use prost::Message as ProstMessage;
use solana_ibc_types::{MsgSendPacket, Payload};

/// Send a GMP call packet
#[derive(Accounts)]
#[instruction(msg: SendCallMsg)]
pub struct SendCall<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Sender of the call
    pub sender: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    /// Router program for sending packets
    /// CHECK: Validated against `app_state`
    #[account(
        constraint = router_program.key() == app_state.router_program @ GMPError::InvalidRouter
    )]
    pub router_program: AccountInfo<'info>,

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

    /// Router caller PDA that represents our app
    /// CHECK: This is a PDA derived with `router_caller` seeds
    #[account(
        seeds = [b"router_caller"],
        bump,
    )]
    pub router_caller: AccountInfo<'info>,

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
    let app_state = &mut ctx.accounts.app_state;

    // Check if app is operational
    app_state.can_operate()?;

    // Validate message
    msg.validate(current_time)?;

    // Create protobuf packet data (matching Ethereum format - no client_id)
    // Note: Empty receiver (system program / all zeros) indicates Cosmos SDK message execution
    let receiver_str = if msg.receiver == Pubkey::default() {
        String::new() // Empty string for Cosmos SDK messages
    } else {
        msg.receiver.to_string()
    };

    let proto_packet_data = GmpPacketData {
        sender: ctx.accounts.sender.key().to_string(),
        receiver: receiver_str,
        salt: msg.salt.clone(),
        payload: msg.payload.clone(),
        memo: msg.memo.clone(),
    };

    // Encode using protobuf
    let mut packet_data_bytes = Vec::new();
    proto_packet_data
        .encode(&mut packet_data_bytes)
        .map_err(|_| GMPError::InvalidPacketData)?;

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
        source_client: msg.source_client.clone(),
        timeout_timestamp: msg.timeout_timestamp,
        payload: ibc_payload,
    };

    // Call router via CPI to actually send the packet
    let sequence = crate::router_cpi::send_packet_cpi(
        &ctx.accounts.router_program,
        &ctx.accounts.router_state,
        &ctx.accounts.client_sequence,
        &ctx.accounts.packet_commitment,
        &ctx.accounts.router_caller.to_account_info(),
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.ibc_app,
        &ctx.accounts.client,
        &ctx.accounts.system_program.to_account_info(),
        router_msg,
        ctx.bumps.router_caller,
    )?;

    // Emit event
    emit!(GMPCallSent {
        sequence,
        sender: ctx.accounts.sender.key(),
        receiver: msg.receiver,
        client_id: msg.source_client,
        salt: msg.salt,
        payload_size: msg.payload.len() as u64,
        timeout_timestamp: msg.timeout_timestamp,
    });

    msg!(
        "GMP call sent: sender={}, receiver={}, sequence={}",
        ctx.accounts.sender.key(),
        msg.receiver,
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

    #[test]
    fn test_send_call_app_paused() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let router_state = Pubkey::new_unique();
        let client_sequence = Pubkey::new_unique();
        let packet_commitment = Pubkey::new_unique();
        let ibc_app = Pubkey::new_unique();
        let client = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let (router_caller_pda, _router_caller_bump) =
            Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let msg = SendCallMsg {
            source_client: "cosmoshub-1".to_string(),
            receiver: Pubkey::new_unique(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            timeout_timestamp: 9_999_999_999,
            memo: String::new(),
        };

        let instruction_data = crate::instruction::SendCall { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(packet_commitment, false),
                AccountMeta::new_readonly(router_caller_pda, false),
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(system_program::ID, false),
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
            create_authority_account(sender),
            create_authority_account(payer),
            create_router_program_account(router_program),
            create_authority_account(router_state),
            create_authority_account(client_sequence),
            create_authority_account(packet_commitment),
            create_authority_account(router_caller_pda),
            create_authority_account(ibc_app),
            create_authority_account(client),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail when app is paused"
        );
    }

    #[test]
    fn test_send_call_invalid_timeout() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let router_state = Pubkey::new_unique();
        let client_sequence = Pubkey::new_unique();
        let packet_commitment = Pubkey::new_unique();
        let ibc_app = Pubkey::new_unique();
        let client = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let (router_caller_pda, _router_caller_bump) =
            Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let msg = SendCallMsg {
            source_client: "cosmoshub-1".to_string(),
            receiver: Pubkey::new_unique(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            timeout_timestamp: 1_000_000, // Timeout in the past
            memo: String::new(),
        };

        let instruction_data = crate::instruction::SendCall { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(packet_commitment, false),
                AccountMeta::new_readonly(router_caller_pda, false),
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(system_program::ID, false),
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
            create_authority_account(sender),
            create_authority_account(payer),
            create_router_program_account(router_program),
            create_authority_account(router_state),
            create_authority_account(client_sequence),
            create_authority_account(packet_commitment),
            create_authority_account(router_caller_pda),
            create_authority_account(ibc_app),
            create_authority_account(client),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with timeout in the past"
        );
    }

    #[test]
    fn test_send_call_invalid_app_state_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let router_state = Pubkey::new_unique();
        let client_sequence = Pubkey::new_unique();
        let packet_commitment = Pubkey::new_unique();
        let ibc_app = Pubkey::new_unique();
        let client = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (_correct_app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, port_id.as_bytes()], &crate::ID);

        // Use wrong PDA in instruction
        let wrong_app_state_pda = Pubkey::new_unique();

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let msg = SendCallMsg {
            source_client: "cosmoshub-1".to_string(),
            receiver: Pubkey::new_unique(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            timeout_timestamp: 9_999_999_999,
            memo: String::new(),
        };

        let instruction_data = crate::instruction::SendCall { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(wrong_app_state_pda, false), // Wrong PDA!
                AccountMeta::new_readonly(sender, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(packet_commitment, false),
                AccountMeta::new_readonly(router_caller_pda, false),
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                wrong_app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false,
            ),
            create_authority_account(sender),
            create_authority_account(payer),
            create_router_program_account(router_program),
            create_authority_account(router_state),
            create_authority_account(client_sequence),
            create_authority_account(packet_commitment),
            create_authority_account(router_caller_pda),
            create_authority_account(ibc_app),
            create_authority_account(client),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with invalid app_state PDA"
        );
    }

    #[test]
    fn test_send_call_wrong_router_program() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let correct_router_program = Pubkey::new_unique();
        let wrong_router_program = Pubkey::new_unique(); // Different router!
        let router_state = Pubkey::new_unique();
        let client_sequence = Pubkey::new_unique();
        let packet_commitment = Pubkey::new_unique();
        let ibc_app = Pubkey::new_unique();
        let client = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let msg = SendCallMsg {
            source_client: "cosmoshub-1".to_string(),
            receiver: Pubkey::new_unique(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            timeout_timestamp: 9_999_999_999,
            memo: String::new(),
        };

        let instruction_data = crate::instruction::SendCall { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(wrong_router_program, false), // Wrong router!
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(packet_commitment, false),
                AccountMeta::new_readonly(router_caller_pda, false),
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                correct_router_program, // Stored in state
                authority,
                app_state_bump,
                false,
            ),
            create_authority_account(sender),
            create_authority_account(payer),
            create_router_program_account(wrong_router_program), // Wrong one passed
            create_authority_account(router_state),
            create_authority_account(client_sequence),
            create_authority_account(packet_commitment),
            create_authority_account(router_caller_pda),
            create_authority_account(ibc_app),
            create_authority_account(client),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with wrong router_program"
        );
    }

    #[test]
    fn test_send_call_payload_too_large() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let router_state = Pubkey::new_unique();
        let client_sequence = Pubkey::new_unique();
        let packet_commitment = Pubkey::new_unique();
        let ibc_app = Pubkey::new_unique();
        let client = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let msg = SendCallMsg {
            source_client: "cosmoshub-1".to_string(),
            receiver: Pubkey::new_unique(),
            salt: vec![1, 2, 3],
            payload: vec![0; crate::constants::MAX_PAYLOAD_LENGTH + 1], // Exceeds limit!
            timeout_timestamp: 9_999_999_999,
            memo: String::new(),
        };

        let instruction_data = crate::instruction::SendCall { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(packet_commitment, false),
                AccountMeta::new_readonly(router_caller_pda, false),
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false,
            ),
            create_authority_account(sender),
            create_authority_account(payer),
            create_router_program_account(router_program),
            create_authority_account(router_state),
            create_authority_account(client_sequence),
            create_authority_account(packet_commitment),
            create_authority_account(router_caller_pda),
            create_authority_account(ibc_app),
            create_authority_account(client),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with payload too large"
        );
    }

    #[test]
    fn test_send_call_empty_payload() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let router_state = Pubkey::new_unique();
        let client_sequence = Pubkey::new_unique();
        let packet_commitment = Pubkey::new_unique();
        let ibc_app = Pubkey::new_unique();
        let client = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let msg = SendCallMsg {
            source_client: "cosmoshub-1".to_string(),
            receiver: Pubkey::new_unique(),
            salt: vec![1, 2, 3],
            payload: vec![], // Empty payload!
            timeout_timestamp: 9_999_999_999,
            memo: String::new(),
        };

        let instruction_data = crate::instruction::SendCall { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(packet_commitment, false),
                AccountMeta::new_readonly(router_caller_pda, false),
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false,
            ),
            create_authority_account(sender),
            create_authority_account(payer),
            create_router_program_account(router_program),
            create_authority_account(router_state),
            create_authority_account(client_sequence),
            create_authority_account(packet_commitment),
            create_authority_account(router_caller_pda),
            create_authority_account(ibc_app),
            create_authority_account(client),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with empty payload"
        );
    }

    #[test]
    fn test_send_call_empty_client_id() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let sender = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let router_state = Pubkey::new_unique();
        let client_sequence = Pubkey::new_unique();
        let packet_commitment = Pubkey::new_unique();
        let ibc_app = Pubkey::new_unique();
        let client = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let msg = SendCallMsg {
            source_client: String::new(), // Empty client ID!
            receiver: Pubkey::new_unique(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            timeout_timestamp: 9_999_999_999,
            memo: String::new(),
        };

        let instruction_data = crate::instruction::SendCall { msg };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(sender, true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_program, false),
                AccountMeta::new_readonly(router_state, false),
                AccountMeta::new(client_sequence, false),
                AccountMeta::new(packet_commitment, false),
                AccountMeta::new_readonly(router_caller_pda, false),
                AccountMeta::new_readonly(ibc_app, false),
                AccountMeta::new_readonly(client, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false,
            ),
            create_authority_account(sender),
            create_authority_account(payer),
            create_router_program_account(router_program),
            create_authority_account(router_state),
            create_authority_account(client_sequence),
            create_authority_account(packet_commitment),
            create_authority_account(router_caller_pda),
            create_authority_account(ibc_app),
            create_authority_account(client),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "SendCall should fail with empty client_id"
        );
    }
}
