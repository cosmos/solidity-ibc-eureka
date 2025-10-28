use crate::constants::IBC_CPI_INSTRUCTION_CAPACITY;
use crate::errors::RouterError;
use crate::state::Packet;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::get_return_data;
use anchor_lang::solana_program::program::invoke;
use solana_ibc_types::{
    ibc_app_instructions, OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg, Payload,
};

// TODO: Params struct
/// CPI helper for calling IBC app's `on_recv_packet` instruction
#[allow(clippy::too_many_arguments)]
pub fn on_recv_packet_cpi<'a>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    packet: &Packet,
    payload: &Payload,
    relayer: &Pubkey,
    remaining_accounts: &[AccountInfo<'a>],
) -> Result<Vec<u8>> {
    let msg = OnRecvPacketMsg {
        source_client: packet.source_client.clone(),
        dest_client: packet.dest_client.clone(),
        sequence: packet.sequence,
        payload: payload.clone(),
        relayer: *relayer,
    };

    call_ibc_app_cpi(
        ibc_app_program,
        app_state,
        router_program,
        payer,
        system_program,
        ibc_app_instructions::on_recv_packet_discriminator(),
        msg,
        remaining_accounts,
    )?;

    // Get the return data (acknowledgement)
    if let Some((program_id, return_data)) = get_return_data() {
        if program_id == *ibc_app_program.key {
            Ok(return_data)
        } else {
            Err(RouterError::InvalidAppResponse.into())
        }
    } else {
        Ok(vec![])
    }
}

// TODO: Params struct
/// CPI helper for calling IBC app's `on_acknowledgement_packet` instruction
#[allow(clippy::too_many_arguments)]
pub fn on_acknowledgement_packet_cpi<'a>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    packet: &Packet,
    payload: &Payload,
    acknowledgement: &[u8],
    relayer: &Pubkey,
    remaining_accounts: &[AccountInfo<'a>],
) -> Result<()> {
    let msg = OnAcknowledgementPacketMsg {
        source_client: packet.source_client.clone(),
        dest_client: packet.dest_client.clone(),
        sequence: packet.sequence,
        payload: payload.clone(),
        acknowledgement: acknowledgement.to_vec(),
        relayer: *relayer,
    };

    call_ibc_app_cpi(
        ibc_app_program,
        app_state,
        router_program,
        payer,
        system_program,
        ibc_app_instructions::on_acknowledgement_packet_discriminator(),
        msg,
        remaining_accounts,
    )
}

// TODO: Params struct
/// CPI helper for calling IBC app's `on_timeout_packet` instruction
#[allow(clippy::too_many_arguments)]
pub fn on_timeout_packet_cpi<'a>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    packet: &Packet,
    payload: &Payload,
    relayer: &Pubkey,
    remaining_accounts: &[AccountInfo<'a>],
) -> Result<()> {
    let msg = OnTimeoutPacketMsg {
        source_client: packet.source_client.clone(),
        dest_client: packet.dest_client.clone(),
        sequence: packet.sequence,
        payload: payload.clone(),
        relayer: *relayer,
    };

    call_ibc_app_cpi(
        ibc_app_program,
        app_state,
        router_program,
        payer,
        system_program,
        ibc_app_instructions::on_timeout_packet_discriminator(),
        msg,
        remaining_accounts,
    )
}

/// Generic CPI helper for calling IBC app instructions
#[allow(clippy::too_many_arguments)]
fn call_ibc_app_cpi<'a, T: AnchorSerialize>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    discriminator: [u8; 8],
    msg: T,
    remaining_accounts: &[AccountInfo<'a>],
) -> Result<()> {
    let mut instruction_data = Vec::with_capacity(IBC_CPI_INSTRUCTION_CAPACITY);
    instruction_data.extend_from_slice(&discriminator);
    msg.serialize(&mut instruction_data)?;

    // Create the instruction with fixed accounts plus remaining accounts
    let mut account_metas = vec![
        AccountMeta::new(*app_state.key, false),
        AccountMeta::new_readonly(*router_program.key, false),
        AccountMeta::new(*payer.key, true),
        AccountMeta::new_readonly(*system_program.key, false),
    ];

    // Add remaining accounts to instruction
    for account_info in remaining_accounts {
        account_metas.push(AccountMeta {
            pubkey: *account_info.key,
            is_signer: account_info.is_signer,
            is_writable: account_info.is_writable,
        });
    }

    let instruction = Instruction {
        program_id: *ibc_app_program.key,
        accounts: account_metas,
        data: instruction_data,
    };

    // Build account_infos array with both fixed and remaining accounts
    let mut account_infos = vec![
        app_state.clone(),
        router_program.clone(),
        payer.clone(),
        system_program.clone(),
    ];
    account_infos.extend_from_slice(remaining_accounts);

    invoke(&instruction, &account_infos)?;

    Ok(())
}
