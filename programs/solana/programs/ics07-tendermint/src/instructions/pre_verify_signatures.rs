use crate::error::ErrorCode;
use crate::state::SignatureVerification;
use crate::PreVerifySignatures;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions as ix_sysvar;
use anchor_lang::solana_program::{program::invoke, system_instruction};
use solana_ibc_types::ics07::SignatureData;

/// Compute hash of signature data for PDA derivation
fn compute_signature_hash(sig_data: &SignatureData) -> [u8; 32] {
    use anchor_lang::solana_program::hash::hashv;
    hashv(&[&sig_data.pubkey, sig_data.msg.as_slice(), &sig_data.signature]).to_bytes()
}

pub fn pre_verify_signatures<'info>(
    ctx: Context<'_, '_, '_, 'info, PreVerifySignatures<'info>>,
    signatures: Vec<SignatureData>,
) -> Result<()> {
    let ix_sysvar_account = &ctx.accounts.instructions_sysvar;
    let current_ix_index = ix_sysvar::load_current_index_checked(ix_sysvar_account)?;

    require!(
        ctx.remaining_accounts.len() == signatures.len(),
        ErrorCode::InvalidNumberOfAccounts
    );

    for (idx, sig_data) in signatures.iter().enumerate() {
        let sig_verification_account = &ctx.remaining_accounts[idx];

        // Compute and verify PDA address
        let sig_hash = compute_signature_hash(sig_data);
        let (expected_pda, _bump) = Pubkey::find_program_address(
            &[SignatureVerification::SEED, &sig_hash],
            ctx.program_id,
        );

        require!(
            sig_verification_account.key() == expected_pda,
            ErrorCode::AccountValidationFailed
        );

        // Verify signature using Ed25519Program
        let is_valid = verify_ed25519_from_sysvar(
            ix_sysvar_account,
            current_ix_index,
            &sig_data.pubkey,
            &sig_data.msg,
            &sig_data.signature,
        )?;

        // Create the PDA account if it doesn't exist
        if sig_verification_account.data_is_empty() {
            let space = 8 + std::mem::size_of::<SignatureVerification>();
            let rent = Rent::get()?.minimum_balance(space);

            invoke(
                &system_instruction::create_account(
                    ctx.accounts.payer.key,
                    sig_verification_account.key,
                    rent,
                    space as u64,
                    ctx.program_id,
                ),
                &[
                    ctx.accounts.payer.to_account_info(),
                    sig_verification_account.clone(),
                    ctx.accounts.system_program.to_account_info(),
                ],
            )?;
        }

        // Serialize and write the verification result
        let verification = SignatureVerification { is_valid };
        let mut data = sig_verification_account.try_borrow_mut_data()?;

        // Anchor's try_serialize includes the discriminator automatically
        verification.try_serialize(&mut data.as_mut())?;
    }

    Ok(())
}

// Ed25519Program instruction format offsets
const ED25519_NUM_SIGNATURES_OFFSET: usize = 0;
const ED25519_SIGNATURE_OFFSET: usize = 1;
const ED25519_PUBKEY_OFFSET: usize = 5;
const ED25519_MESSAGE_OFFSET: usize = 9;
const ED25519_MESSAGE_SIZE_OFFSET: usize = 11;
const ED25519_HEADER_SIZE: usize = 13;

/// Verify signature by checking Ed25519Program instruction in sysvar
fn verify_ed25519_from_sysvar(
    ix_sysvar_account: &AccountInfo,
    current_index: u16,
    pubkey: &[u8; 32],
    msg: &[u8],
    signature: &[u8; 64],
) -> Result<bool> {
    // Search all instructions before current one
    for i in 0..current_index {
        let ix = ix_sysvar::load_instruction_at_checked(i as usize, ix_sysvar_account)?;

        // Skip non-Ed25519Program instructions
        if ix.program_id != anchor_lang::solana_program::ed25519_program::ID {
            continue;
        }

        // Verify minimum size and single signature
        if ix.data.len() < ED25519_HEADER_SIZE + 64 + 32
            || ix.data[ED25519_NUM_SIGNATURES_OFFSET] != 1
        {
            continue;
        }

        // Parse offsets from instruction data
        let sig_offset =
            u16::from_le_bytes([ix.data[ED25519_SIGNATURE_OFFSET], ix.data[ED25519_SIGNATURE_OFFSET + 1]]) as usize;
        let pubkey_offset =
            u16::from_le_bytes([ix.data[ED25519_PUBKEY_OFFSET], ix.data[ED25519_PUBKEY_OFFSET + 1]]) as usize;
        let msg_offset =
            u16::from_le_bytes([ix.data[ED25519_MESSAGE_OFFSET], ix.data[ED25519_MESSAGE_OFFSET + 1]]) as usize;
        let msg_size = u16::from_le_bytes([
            ix.data[ED25519_MESSAGE_SIZE_OFFSET],
            ix.data[ED25519_MESSAGE_SIZE_OFFSET + 1],
        ]) as usize;

        // Validate bounds
        if sig_offset + 64 > ix.data.len()
            || pubkey_offset + 32 > ix.data.len()
            || msg_offset + msg_size > ix.data.len()
        {
            continue;
        }

        // Extract and compare signature components
        if &ix.data[sig_offset..sig_offset + 64] == signature
            && &ix.data[pubkey_offset..pubkey_offset + 32] == pubkey
            && &ix.data[msg_offset..msg_offset + msg_size] == msg
        {
            // Ed25519Program already verified this signature
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests;
