use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions as ix_sysvar;

use crate::PreVerifySignature;
use solana_ibc_types::ics07::SignatureData;

pub fn pre_verify_signature<'info>(
    ctx: Context<'_, '_, '_, 'info, PreVerifySignature<'info>>,
    signature: SignatureData,
) -> Result<()> {
    let ix_sysvar = &ctx.accounts.instructions_sysvar;
    let current_ix_index = ix_sysvar::load_current_index_checked(ix_sysvar)?;

    // Verify the signature using Ed25519Program instruction in sysvar
    let is_valid = verify_ed25519_from_sysvar(
        ix_sysvar,
        current_ix_index,
        &signature.pubkey,
        &signature.msg,
        &signature.signature,
    )?;

    // Store verification result (account already initialized by Anchor's init constraint)
    ctx.accounts.signature_verification.is_valid = is_valid;

    Ok(())
}

/// Ed25519Program instruction format offsets
const ED25519_NUM_SIGNATURES_OFFSET: usize = 0;
const ED25519_SIGNATURE_OFFSET: usize = 1;
const ED25519_PUBKEY_OFFSET: usize = 5;
const ED25519_MESSAGE_OFFSET: usize = 9;
const ED25519_MESSAGE_SIZE_OFFSET: usize = 11;
const ED25519_HEADER_SIZE: usize = 13;

/// Verify signature by finding matching Ed25519Program instruction in the sysvar.
/// Returns true if a prior Ed25519Program instruction already verified this signature.
fn verify_ed25519_from_sysvar(
    ix_sysvar: &AccountInfo,
    current_index: u16,
    pubkey: &[u8; 32],
    msg: &[u8],
    signature: &[u8; 64],
) -> Result<bool> {
    for i in 0..current_index {
        let ix = ix_sysvar::load_instruction_at_checked(i as usize, ix_sysvar)?;

        if ix.program_id != anchor_lang::solana_program::ed25519_program::ID {
            continue;
        }

        if ix.data.len() < ED25519_HEADER_SIZE + 96 || ix.data[ED25519_NUM_SIGNATURES_OFFSET] != 1 {
            continue;
        }

        let sig_offset = u16::from_le_bytes([
            ix.data[ED25519_SIGNATURE_OFFSET],
            ix.data[ED25519_SIGNATURE_OFFSET + 1],
        ]) as usize;
        let pubkey_offset = u16::from_le_bytes([
            ix.data[ED25519_PUBKEY_OFFSET],
            ix.data[ED25519_PUBKEY_OFFSET + 1],
        ]) as usize;
        let msg_offset = u16::from_le_bytes([
            ix.data[ED25519_MESSAGE_OFFSET],
            ix.data[ED25519_MESSAGE_OFFSET + 1],
        ]) as usize;
        let msg_size = u16::from_le_bytes([
            ix.data[ED25519_MESSAGE_SIZE_OFFSET],
            ix.data[ED25519_MESSAGE_SIZE_OFFSET + 1],
        ]) as usize;

        if sig_offset + 64 > ix.data.len()
            || pubkey_offset + 32 > ix.data.len()
            || msg_offset + msg_size > ix.data.len()
        {
            continue;
        }

        if &ix.data[sig_offset..sig_offset + 64] == signature
            && &ix.data[pubkey_offset..pubkey_offset + 32] == pubkey
            && &ix.data[msg_offset..msg_offset + msg_size] == msg
        {
            return Ok(true);
        }
    }

    Ok(false)
}
