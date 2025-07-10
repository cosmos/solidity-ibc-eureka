#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;

pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;
pub mod types;

use crate::state::{ClientData, ConsensusStateStore};

declare_id!("8wQAC7oWLTxExhR49jYAzXZB39mu7WVVvkWJGgAMMjpV");

pub use types::{ClientState, ConsensusState, UpdateClientMsg, MembershipMsg, MisbehaviourMsg};

#[derive(Accounts)]
#[instruction(chain_id: String)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + 1000,
        seeds = [b"client", chain_id.as_bytes()],
        bump
    )]
    pub client_data: Account<'info, ClientData>,
    #[account(
        init,
        payer = payer,
        space = 8 + 8 + 8 + 32 + 32, // discriminator + height + timestamp + root + next_validators_hash
        seeds = [b"consensus_state", client_data.key().as_ref(), &0u64.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(msg: UpdateClientMsg)]
pub struct UpdateClient<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + 8 + 8 + 32 + 32,
        seeds = [b"consensus_state", client_data.key().as_ref(), &client_data.client_state.latest_height.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct VerifyMembership<'info> {
    pub client_data: Account<'info, ClientData>,
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
pub struct VerifyNonMembership<'info> {
    pub client_data: Account<'info, ClientData>,
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
#[instruction(msg: MisbehaviourMsg)]
pub struct SubmitMisbehaviour<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
    pub trusted_consensus_state_1: Account<'info, ConsensusStateStore>,
    pub trusted_consensus_state_2: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
pub struct GetConsensusStateHash<'info> {
    pub client_data: Account<'info, ClientData>,
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
}

#[program]
pub mod ics07_tendermint {
    use super::*;
    use crate::types::{ClientState, ConsensusState, UpdateClientMsg, MembershipMsg, MisbehaviourMsg};

    pub fn initialize(
        ctx: Context<Initialize>,
        _chain_id: String,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> Result<()> {
        // NOTE: chain_id is used in the #[instruction] attribute for account validation
        // but the actual handler doesn't need it as it's embedded in client_state
        instructions::initialize::initialize(ctx, client_state, consensus_state)
    }

    pub fn update_client(
        ctx: Context<UpdateClient>,
        msg: UpdateClientMsg
    ) -> Result<()> {
        instructions::update_client::update_client(ctx, msg)
    }

    pub fn verify_membership(
        ctx: Context<VerifyMembership>,
        msg: MembershipMsg
    ) -> Result<()> {
        instructions::verify_membership::verify_membership(ctx, msg)
    }

    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: MembershipMsg,
    ) -> Result<()> {
        instructions::verify_non_membership::verify_non_membership(ctx, msg)
    }

    pub fn submit_misbehaviour(
        ctx: Context<SubmitMisbehaviour>,
        msg: MisbehaviourMsg,
    ) -> Result<()> {
        instructions::submit_misbehaviour::submit_misbehaviour(ctx, msg)
    }

    pub fn get_consensus_state_hash(
        ctx: Context<GetConsensusStateHash>,
        revision_height: u64,
    ) -> Result<[u8; 32]> {
        instructions::get_consensus_state_hash::get_consensus_state_hash(ctx, revision_height)
    }
}
