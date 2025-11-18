use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use anchor_lang::solana_program::sysvar::instructions as ix_sysvar;
use anchor_lang::solana_program::{program::invoke, system_instruction};

use crate::error::ErrorCode;
use crate::state::SignatureVerification;
use crate::PreVerifySignatures;
use solana_ibc_types::ics07::SignatureData;

fn compute_signature_hash(sig_data: &SignatureData) -> [u8; 32] {
    hashv(&[&sig_data.pubkey, sig_data.msg.as_slice(), &sig_data.signature]).to_bytes()
}

pub fn pre_verify_signatures<'info>(
    ctx: Context<'_, '_, '_, 'info, PreVerifySignatures<'info>>,
    signatures: Vec<SignatureData>,
) -> Result<()> {
    let ix_sysvar = &ctx.accounts.instructions_sysvar;
    let current_ix_index = ix_sysvar::load_current_index_checked(ix_sysvar)?;

    require!(
        ctx.remaining_accounts.len() == signatures.len(),
        ErrorCode::InvalidNumberOfAccounts
    );

    for (idx, sig_data) in signatures.iter().enumerate() {
        let account = &ctx.remaining_accounts[idx];

        let sig_hash = compute_signature_hash(sig_data);
        let (expected_pda, _) = Pubkey::find_program_address(
            &[SignatureVerification::SEED, &sig_hash],
            ctx.program_id,
        );

        require!(
            account.key() == expected_pda,
            ErrorCode::AccountValidationFailed
        );

        let is_valid = verify_ed25519_from_sysvar(
            ix_sysvar,
            current_ix_index,
            &sig_data.pubkey,
            &sig_data.msg,
            &sig_data.signature,
        )?;

        if account.data_is_empty() {
            let space = 8 + std::mem::size_of::<SignatureVerification>();
            let rent = Rent::get()?.minimum_balance(space);

            invoke(
                &system_instruction::create_account(
                    ctx.accounts.payer.key,
                    account.key,
                    rent,
                    space as u64,
                    ctx.program_id,
                ),
                &[
                    ctx.accounts.payer.to_account_info(),
                    account.clone(),
                    ctx.accounts.system_program.to_account_info(),
                ],
            )?;
        }

        let verification = SignatureVerification { is_valid };
        let mut data = account.try_borrow_mut_data()?;
        verification.try_serialize(&mut data.as_mut())?;
    }

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

#[cfg(test)]
mod tests;
