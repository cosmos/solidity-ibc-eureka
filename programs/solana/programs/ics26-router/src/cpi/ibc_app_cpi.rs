use crate::errors::RouterError;
use crate::state::Packet;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::get_return_data;
use anchor_lang::solana_program::program::invoke;
use ics24_host_solana::{
    OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg, Payload,
};

/// CPI helper for calling IBC app's `on_recv_packet` instruction
pub fn on_recv_packet_cpi<'a>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    packet: &Packet,
    payload: &Payload,
    relayer: &Pubkey,
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
        "global:on_recv_packet",
        msg,
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

/// CPI helper for calling IBC app's `on_acknowledgement_packet` instruction
pub fn on_acknowledgement_packet_cpi<'a>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    packet: &Packet,
    payload: &Payload,
    acknowledgement: &[u8],
    relayer: &Pubkey,
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
        "global:on_acknowledgement_packet",
        msg,
    )
}

/// CPI helper for calling IBC app's `on_timeout_packet` instruction
pub fn on_timeout_packet_cpi<'a>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    packet: &Packet,
    payload: &Payload,
    relayer: &Pubkey,
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
        "global:on_timeout_packet",
        msg,
    )
}

/// Generic CPI helper for calling IBC app instructions
fn call_ibc_app_cpi<'a, T: AnchorSerialize>(
    ibc_app_program: &AccountInfo<'a>,
    app_state: &AccountInfo<'a>,
    router_program: &AccountInfo<'a>,
    discriminator: &str,
    msg: T,
) -> Result<()> {
    // Encode the instruction data using Anchor's discriminator
    let mut instruction_data = Vec::with_capacity(256);
    instruction_data.extend_from_slice(
        &anchor_lang::solana_program::hash::hash(discriminator.as_bytes()).to_bytes()[..8],
    );
    msg.serialize(&mut instruction_data)?;

    // Create the instruction
    let instruction = Instruction {
        program_id: *ibc_app_program.key,
        accounts: vec![
            AccountMeta::new(*app_state.key, false),
            AccountMeta::new_readonly(*router_program.key, false), // router_program account
        ],
        data: instruction_data,
    };

    // Invoke the CPI
    let account_infos = &[
        app_state.clone(),
        router_program.clone(), // Pass the router program for auth check
    ];
    invoke(&instruction, account_infos)?;

    Ok(())
}
