use crate::errors::RouterError;
use crate::state::{Commitment, RouterState, COMMITMENT_SEED, ROUTER_STATE_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(path_hash: [u8; 32])]
pub struct StoreCommitment<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        init,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE , // discriminator + commitment
        seeds = [COMMITMENT_SEED, &path_hash],
        bump
    )]
    pub commitment: Account<'info, Commitment>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(path_hash: [u8; 32])]
pub struct GetCommitment<'info> {
    #[account(
        seeds = [COMMITMENT_SEED, &path_hash],
        bump
    )]
    pub commitment: Account<'info, Commitment>,
}

pub fn store_commitment(
    ctx: Context<StoreCommitment>,
    _path_hash: [u8; 32],
    commitment: [u8; 32],
) -> Result<()> {
    let router_state = &ctx.accounts.router_state;

    require!(
        ctx.accounts.authority.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    let commitment_account = &mut ctx.accounts.commitment;
    commitment_account.value = commitment;

    Ok(())
}

pub fn get_commitment(ctx: Context<GetCommitment>, _path_hash: [u8; 32]) -> Result<[u8; 32]> {
    Ok(ctx.accounts.commitment.value)
}

