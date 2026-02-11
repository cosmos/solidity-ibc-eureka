use anchor_lang::prelude::*;
use anchor_lang::Discriminator;

/// Context for cleaning up incomplete header uploads or signatures
#[derive(Accounts)]
pub struct CleanupIncompleteUpload<'info> {
    /// The original submitter who gets their rent back
    /// Must be the signer to prove they own the upload
    #[account(mut)]
    pub submitter: Signer<'info>,
    // Remaining accounts are the chunk and signature verification accounts to close
}

/// Cleans up incomplete update client uploads by closing both `HeaderChunk` and `SignatureVerification` PDAs
pub fn cleanup_incomplete_upload(ctx: Context<CleanupIncompleteUpload>) -> Result<()> {
    let submitter = ctx.accounts.submitter.key();

    for account in ctx.remaining_accounts {
        if account.owner != &crate::ID || account.lamports() == 0 {
            continue;
        }

        let should_close = is_owned_by_submitter(account, submitter)?;

        if should_close {
            crate::helpers::close_account(account, &ctx.accounts.submitter)?;
        }
    }

    Ok(())
}

/// Checks if an account is a `HeaderChunk` or `SignatureVerification` owned by the given submitter
pub(crate) fn is_owned_by_submitter(account: &AccountInfo, submitter: Pubkey) -> Result<bool> {
    let data = account.try_borrow_data()?;

    if data.len() < 8 {
        return Ok(false);
    }

    let disc = &data[..8];

    if disc == crate::state::HeaderChunk::DISCRIMINATOR {
        return Ok(
            anchor_lang::AccountDeserialize::try_deserialize(&mut &data[..])
                .is_ok_and(|chunk: crate::state::HeaderChunk| chunk.submitter == submitter),
        );
    }

    if disc == crate::state::SignatureVerification::DISCRIMINATOR {
        return Ok(
            anchor_lang::AccountDeserialize::try_deserialize(&mut &data[..])
                .is_ok_and(|sig: crate::state::SignatureVerification| sig.submitter == submitter),
        );
    }

    Ok(false)
}

#[cfg(test)]
mod tests;
