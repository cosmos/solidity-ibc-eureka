use anchor_lang::prelude::*;
use ics26_router::cpi::accounts::SendPacket;
use solana_ibc_types::MsgSendPacket;

/// Send IBC packet via CPI to the ICS26 router
/// This function creates and sends a GMP packet from Solana to another chain
#[allow(clippy::too_many_arguments)]
pub fn send_packet_cpi<'a>(
    router_program: &AccountInfo<'a>,
    router_state: &AccountInfo<'a>,
    client_sequence: &AccountInfo<'a>,
    packet_commitment: &AccountInfo<'a>,
    instruction_sysvar: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    ibc_app: &AccountInfo<'a>,
    client: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    msg: MsgSendPacket,
) -> Result<u64> {
    let cpi_accounts = SendPacket {
        router_state: router_state.clone(),
        ibc_app: ibc_app.clone(),
        client_sequence: client_sequence.clone(),
        packet_commitment: packet_commitment.clone(),
        instruction_sysvar: instruction_sysvar.clone(),
        payer: payer.clone(),
        system_program: system_program.clone(),
        client: client.clone(),
    };

    let cpi_ctx = CpiContext::new(router_program.clone(), cpi_accounts);
    let sequence = ics26_router::cpi::send_packet(cpi_ctx, msg)?;
    Ok(sequence.get())
}
