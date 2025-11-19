use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions as ix_sysvar;

use crate::PreVerifySignature;
use solana_ibc_types::ics07::SignatureData;

pub fn pre_verify_signature<'info>(
    ctx: Context<'_, '_, '_, 'info, PreVerifySignature<'info>>,
    signature: SignatureData,
) -> Result<()> {
    let ix_sysvar = &ctx.accounts.instructions_sysvar;

    let is_valid = verify_ed25519_from_sysvar(
        ix_sysvar,
        &signature.pubkey,
        &signature.msg,
        &signature.signature,
    )?;

    ctx.accounts.signature_verification.is_valid = is_valid;

    Ok(())
}

const ED25519_NUM_SIGNATURES_OFFSET: usize = 0;
const ED25519_SIGNATURE_OFFSET: usize = 2;
const ED25519_PUBKEY_OFFSET: usize = 6;
const ED25519_MESSAGE_OFFSET: usize = 10;
const ED25519_MESSAGE_SIZE_OFFSET: usize = 12;
const ED25519_HEADER_SIZE: usize = 16;

fn verify_ed25519_from_sysvar(
    ix_sysvar: &AccountInfo,
    pubkey: &[u8; 32],
    msg: &[u8],
    signature: &[u8; 64],
) -> Result<bool> {
    const ED25519_IX_INDEX: usize = 0;

    let ix = ix_sysvar::load_instruction_at_checked(ED25519_IX_INDEX, ix_sysvar)?;

    if ix.program_id != anchor_lang::solana_program::ed25519_program::ID {
        return Ok(false);
    }

    if ix.data.len() < ED25519_HEADER_SIZE + 96 || ix.data[ED25519_NUM_SIGNATURES_OFFSET] != 1 {
        return Ok(false);
    }

    let msg_size = u16::from_le_bytes([
        ix.data[ED25519_MESSAGE_SIZE_OFFSET],
        ix.data[ED25519_MESSAGE_SIZE_OFFSET + 1],
    ]) as usize;

    if msg_size != msg.len() {
        return Ok(false);
    }

    let pubkey_offset = u16::from_le_bytes([
        ix.data[ED25519_PUBKEY_OFFSET],
        ix.data[ED25519_PUBKEY_OFFSET + 1],
    ]) as usize;
    let sig_offset = u16::from_le_bytes([
        ix.data[ED25519_SIGNATURE_OFFSET],
        ix.data[ED25519_SIGNATURE_OFFSET + 1],
    ]) as usize;
    let msg_offset = u16::from_le_bytes([
        ix.data[ED25519_MESSAGE_OFFSET],
        ix.data[ED25519_MESSAGE_OFFSET + 1],
    ]) as usize;

    if sig_offset + 64 > ix.data.len()
        || pubkey_offset + 32 > ix.data.len()
        || msg_offset + msg_size > ix.data.len()
    {
        return Ok(false);
    }

    if &ix.data[pubkey_offset..pubkey_offset + 32] != pubkey {
        return Ok(false);
    }
    if &ix.data[sig_offset..sig_offset + 64] != signature {
        return Ok(false);
    }
    if &ix.data[msg_offset..msg_offset + msg_size] != msg {
        return Ok(false);
    }

    Ok(true)
}
