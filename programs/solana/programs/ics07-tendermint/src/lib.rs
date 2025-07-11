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

pub use types::{ClientState, ConsensusState, UpdateClientMsg, MembershipMsg, MisbehaviourMsg, UpdateResult};

#[derive(Accounts)]
#[instruction(chain_id: String, client_state: ClientState)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ClientData::INIT_SPACE,
        seeds = [b"client", chain_id.as_bytes()],
        bump
    )]
    pub client_data: Account<'info, ClientData>,
    #[account(
        init,
        payer = payer,
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [b"consensus_state", client_data.key().as_ref(), &client_state.latest_height.revision_height.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateClient<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
    /// Consensus state store for the new height
    /// Will be created if it doesn't exist, or validated if it does (for misbehaviour detection)
    /// NOTE: We can't use the instruction parameter here because we don't know the new height
    /// until after processing the update. This account must be derived by the client
    /// based on the expected new height from the header.
    /// CHECK: This account is validated in the instruction handler
    pub new_consensus_state_store: UncheckedAccount<'info>,
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
    ) -> Result<UpdateResult> {
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
}
