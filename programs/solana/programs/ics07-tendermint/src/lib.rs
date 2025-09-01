use anchor_lang::prelude::*;

pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg(test)]
pub mod test_helpers;
pub mod types;

use crate::state::ConsensusStateStore;

declare_id!("HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD");

pub use types::{
    ClientState, ConsensusState, IbcHeight, MisbehaviourMsg, UpdateClientMsg, UpdateResult,
};

pub use ics25_handler::MembershipMsg;

#[derive(Accounts)]
#[instruction(latest_height: u64, client_state: ClientState)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ClientState::INIT_SPACE,
        seeds = [b"client"],  // Single client per program instance
        bump
    )]
    pub client_state: Account<'info, ClientState>,
    #[account(
        init,
        payer = payer,
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [b"consensus_state", latest_height.to_le_bytes().as_ref()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateClient<'info> {
    #[account(
        mut,
        seeds = [b"client"],
        bump
    )]
    pub client_state: Account<'info, ClientState>,
    /// Trusted consensus state at the height specified in the header
    /// CHECK: This account is validated in the instruction handler based on the trusted height from the header
    pub trusted_consensus_state: UncheckedAccount<'info>,
    /// Consensus state store for the new height - will be created or validated
    /// CHECK: This account is validated in the instruction handler
    pub new_consensus_state_store: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct VerifyMembership<'info> {
    pub client_state: Account<'info, ClientState>,
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
pub struct VerifyNonMembership<'info> {
    pub client_state: Account<'info, ClientState>,
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
#[instruction(msg: MisbehaviourMsg)]
pub struct SubmitMisbehaviour<'info> {
    #[account(mut)]
    pub client_state: Account<'info, ClientState>,
    pub trusted_consensus_state_1: Account<'info, ConsensusStateStore>,
    pub trusted_consensus_state_2: Account<'info, ConsensusStateStore>,
}

#[program]
pub mod ics07_tendermint {
    use super::*;
    use crate::types::{ClientState, ConsensusState, MisbehaviourMsg, UpdateClientMsg};

    pub fn initialize(
        ctx: Context<Initialize>,
        latest_height: u64,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> Result<()> {
        // Validate that the provided height matches client state
        assert_eq!(client_state.latest_height.revision_height, latest_height);

        instructions::initialize::initialize(ctx, client_state, consensus_state)
    }

    pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<UpdateResult> {
        instructions::update_client::update_client(ctx, msg)
    }

    pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
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
