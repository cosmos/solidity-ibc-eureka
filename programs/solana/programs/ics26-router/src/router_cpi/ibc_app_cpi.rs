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

/// Common accounts required for IBC app CPI calls
#[derive(Clone)]
pub struct IbcAppCpiAccounts<'a> {
    pub ibc_app_program: AccountInfo<'a>,
    pub app_state: AccountInfo<'a>,
    pub router_program: AccountInfo<'a>,
    pub payer: AccountInfo<'a>,
    pub system_program: AccountInfo<'a>,
}

// TODO: get payload from packet
/// CPI helper for calling IBC app's `on_recv_packet` instruction
pub fn on_recv_packet_cpi<'a>(
    accounts: IbcAppCpiAccounts<'a>,
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
        &accounts,
        ibc_app_instructions::on_recv_packet_discriminator(),
        msg,
        remaining_accounts,
    )?;

    // Get the return data (acknowledgement)
    if let Some((program_id, return_data)) = get_return_data() {
        if program_id == *accounts.ibc_app_program.key {
            Ok(return_data)
        } else {
            Err(RouterError::InvalidAppResponse.into())
        }
    } else {
        Ok(vec![])
    }
}

/// CPI helper for calling IBC app's `on_acknowledgement_packet` instruction
pub fn on_acknowledgement_packet_cpi<'a>(
    accounts: IbcAppCpiAccounts<'a>,
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
        &accounts,
        ibc_app_instructions::on_acknowledgement_packet_discriminator(),
        msg,
        remaining_accounts,
    )
}

/// CPI helper for calling IBC app's `on_timeout_packet` instruction
pub fn on_timeout_packet_cpi<'a>(
    accounts: IbcAppCpiAccounts<'a>,
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
        &accounts,
        ibc_app_instructions::on_timeout_packet_discriminator(),
        msg,
        remaining_accounts,
    )
}

/// Generic CPI helper for calling IBC app instructions
fn call_ibc_app_cpi<'a, T: AnchorSerialize>(
    accounts: &IbcAppCpiAccounts<'a>,
    discriminator: [u8; 8],
    msg: T,
    remaining_accounts: &[AccountInfo<'a>],
) -> Result<()> {
    let mut instruction_data = Vec::with_capacity(IBC_CPI_INSTRUCTION_CAPACITY);
    instruction_data.extend_from_slice(&discriminator);
    msg.serialize(&mut instruction_data)?;

    // Create the instruction with fixed accounts plus remaining accounts
    let mut account_metas = vec![
        AccountMeta::new(*accounts.app_state.key, false),
        AccountMeta::new_readonly(*accounts.router_program.key, false),
        AccountMeta::new(*accounts.payer.key, true),
        AccountMeta::new_readonly(*accounts.system_program.key, false),
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
        program_id: *accounts.ibc_app_program.key,
        accounts: account_metas,
        data: instruction_data,
    };

    // Build account_infos array with both fixed and remaining accounts
    let mut account_infos = vec![
        accounts.app_state.clone(),
        accounts.router_program.clone(),
        accounts.payer.clone(),
        accounts.system_program.clone(),
    ];
    account_infos.extend_from_slice(remaining_accounts);

    invoke(&instruction, &account_infos)?;

    Ok(())
}
