use crate::errors::GMPError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::Instruction,
    program::{get_return_data, invoke},
};
use solana_ibc_types::MsgSendPacket;
use solana_sha256_hasher::hash;

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
    // Build instruction data with Anchor discriminator
    let mut instruction_data = Vec::with_capacity(256);

    // Anchor instruction discriminator: first 8 bytes of hash of "global:send_packet"
    let discriminator = hash(b"global:send_packet").to_bytes();
    instruction_data.extend_from_slice(&discriminator[..8]);

    // Append serialized MsgSendPacket data
    msg.serialize(&mut instruction_data)?;

    // Build CPI instruction for router's send_packet
    let instruction = Instruction {
        program_id: *router_program.key,
        accounts: vec![
            AccountMeta::new_readonly(*router_state.key, false),
            AccountMeta::new_readonly(*ibc_app.key, false),
            AccountMeta::new(*client_sequence.key, false),
            AccountMeta::new(*packet_commitment.key, false),
            AccountMeta::new_readonly(*instruction_sysvar.key, false), // Instructions sysvar
            AccountMeta::new(*payer.key, true),
            AccountMeta::new_readonly(*system_program.key, false),
            AccountMeta::new_readonly(*client.key, false),
        ],
        data: instruction_data,
    };

    // Execute CPI to router (no PDA signing needed - router validates via instruction sysvar)
    let account_infos = &[
        router_state.clone(),
        ibc_app.clone(),
        client_sequence.clone(),
        packet_commitment.clone(),
        instruction_sysvar.clone(),
        payer.clone(),
        system_program.clone(),
        client.clone(),
    ];

    invoke(&instruction, account_infos)?;

    // Read the namespaced sequence from Router's return data
    // Router returns the actual sequence used for the packet commitment
    if let Some((program_id, data)) = get_return_data() {
        if program_id == *router_program.key && data.len() >= 8 {
            let sequence = u64::from_le_bytes(
                data[..8]
                    .try_into()
                    .map_err(|_| GMPError::SequenceParseError)?,
            );
            return Ok(sequence);
        }
    }
    Err(GMPError::SequenceParseError.into())
}
