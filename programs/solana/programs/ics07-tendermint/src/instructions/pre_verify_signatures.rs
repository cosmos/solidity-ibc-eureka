use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions as ix_sysvar;
use solana_sdk_ids::ed25519_program;

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

    ctx.accounts.signature_verification.submitter = ctx.accounts.payer.key();
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

    if ix.program_id != ed25519_program::ID
        || ix.data.len() < ED25519_HEADER_SIZE + 96
        || ix.data[ED25519_NUM_SIGNATURES_OFFSET] != 1
    {
        return Ok(false);
    }

    // Check msg size before loading offsets
    let msg_size = u16::from_le_bytes([
        ix.data[ED25519_MESSAGE_SIZE_OFFSET],
        ix.data[ED25519_MESSAGE_SIZE_OFFSET + 1],
    ]) as usize;

    if msg_size != msg.len() {
        return Ok(false);
    }

    let offsets = load_offsets(&ix.data);

    // Bounds check
    if offsets.signature + 64 > ix.data.len()
        || offsets.pubkey + 32 > ix.data.len()
        || offsets.msg + msg_size > ix.data.len()
    {
        return Ok(false);
    }

    Ok(&ix.data[offsets.pubkey..offsets.pubkey + 32] == pubkey
        && &ix.data[offsets.signature..offsets.signature + 64] == signature
        && &ix.data[offsets.msg..offsets.msg + msg_size] == msg)
}

#[inline]
fn load_offsets(data: &[u8]) -> Offsets {
    Offsets {
        signature: u16::from_le_bytes([data[2], data[3]]) as usize,
        pubkey: u16::from_le_bytes([data[6], data[7]]) as usize,
        msg: u16::from_le_bytes([data[10], data[11]]) as usize,
    }
}

struct Offsets {
    signature: usize,
    pubkey: usize,
    msg: usize,
}
