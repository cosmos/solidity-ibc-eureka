use anchor_lang::prelude::*;

pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;

use state::{ClientState, ConsensusStateStore, EthereumAddress, UpdateResult};

declare_id!("4AFX7zqsHerxVuZGsNenjenS5R2cYHLmwwx53y6QN8Mk");

pub use ics25_handler::{MembershipMsg, NonMembershipMsg};

#[derive(Accounts)]
#[instruction(client_id: String, attestor_addresses: Vec<EthereumAddress>, min_required_sigs: u8, initial_height: u64, initial_timestamp: u64)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ClientState::INIT_SPACE,
        seeds = [ClientState::SEED, client_id.as_bytes()],
        bump
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(
        init,
        payer = payer,
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [
            ConsensusStateStore::SEED,
            client_state.key().as_ref(),
            &initial_height.to_le_bytes()
        ],
        bump
    )]
    pub initial_consensus_state: Account<'info, ConsensusStateStore>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(msg: ics25_handler::MembershipMsg)]
pub struct VerifyMembership<'info> {
    pub client_state: Account<'info, ClientState>,

    #[account(
        seeds = [
            ConsensusStateStore::SEED,
            client_state.key().as_ref(),
            &msg.height.to_le_bytes()
        ],
        bump
    )]
    pub consensus_state: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
#[instruction(msg: ics25_handler::NonMembershipMsg)]
pub struct VerifyNonMembership<'info> {
    pub client_state: Account<'info, ClientState>,

    #[account(
        seeds = [
            ConsensusStateStore::SEED,
            client_state.key().as_ref(),
            &msg.height.to_le_bytes()
        ],
        bump
    )]
    pub consensus_state: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
#[instruction(update_msg: Vec<u8>)]
pub struct UpdateClient<'info> {
    #[account(mut)]
    pub client_state: Account<'info, ClientState>,

    /// CHECK: PDA validation and initialization handled in handler
    #[account(mut)]
    pub consensus_state: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// TODO: Implement access control
#[program]
pub mod attestation {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        client_id: String,
        attestor_addresses: Vec<EthereumAddress>,
        min_required_sigs: u8,
        initial_height: u64,
        initial_timestamp: u64,
    ) -> Result<()> {
        instructions::initialize::handler(
            ctx,
            client_id,
            attestor_addresses,
            min_required_sigs,
            initial_height,
            initial_timestamp,
        )
    }

    /// Update the client with a new consensus state. Returns UpdateResult
    /// indicating success, no-op, or misbehavior
    pub fn update_client(ctx: Context<UpdateClient>, update_msg: Vec<u8>) -> Result<UpdateResult> {
        instructions::update_client::handler(ctx, update_msg)
    }

    /// Verify membership
    pub fn verify_membership(
        ctx: Context<VerifyMembership>,
        msg: ics25_handler::MembershipMsg,
    ) -> Result<()> {
        instructions::verify_membership::handler(ctx, msg)
    }

    /// Verify non membership
    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: ics25_handler::NonMembershipMsg,
    ) -> Result<()> {
        instructions::verify_non_membership::handler(ctx, msg)
    }
}
