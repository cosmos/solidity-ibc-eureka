use anchor_lang::prelude::*;
use anchor_lang::system_program;

/// Creates a PDA account defensively, tolerating pre-funded addresses.
///
/// Uses transfer + allocate + assign instead of `create_account` to avoid
/// griefing via pre-funded PDAs.
pub fn create_pda_account<'info>(
    payer: &AccountInfo<'info>,
    target: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    owner: &Pubkey,
    space: usize,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(space);
    let current_lamports = target.lamports();

    if current_lamports < required_lamports {
        let delta = required_lamports - current_lamports;
        system_program::transfer(
            CpiContext::new(
                system_program.clone(),
                system_program::Transfer {
                    from: payer.clone(),
                    to: target.clone(),
                },
            ),
            delta,
        )?;
    }

    system_program::allocate(
        CpiContext::new_with_signer(
            system_program.clone(),
            system_program::Allocate {
                account_to_allocate: target.clone(),
            },
            signer_seeds,
        ),
        space as u64,
    )?;

    system_program::assign(
        CpiContext::new_with_signer(
            system_program.clone(),
            system_program::Assign {
                account_to_assign: target.clone(),
            },
            signer_seeds,
        ),
        owner,
    )?;

    Ok(())
}
