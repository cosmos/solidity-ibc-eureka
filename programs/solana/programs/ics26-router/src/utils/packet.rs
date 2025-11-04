use crate::{errors::RouterError, state::IBCApp};
use anchor_lang::prelude::*;
use solana_ibc_types::{Packet, Payload};

// TODO: Support multi-payload packets #602
pub fn get_single_payload(packet: &Packet) -> Result<&Payload> {
    match &packet.payloads[..] {
        [payload] => Ok(payload),
        _ => err!(RouterError::InvalidPayloadCount),
    }
}

pub fn validate_ibc_app_pda(
    program_id: &Pubkey,
    payload: &Payload,
    ibc_app_key: Pubkey,
) -> Result<()> {
    let expected_ibc_app = Pubkey::find_program_address(
        &[IBCApp::SEED, payload.source_port.as_bytes()],
        program_id,
    )
    .0;

    require!(ibc_app_key == expected_ibc_app, RouterError::IbcAppNotFound);
    Ok(())
}
