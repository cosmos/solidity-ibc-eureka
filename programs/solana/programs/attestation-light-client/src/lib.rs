use anchor_lang::prelude::*;

pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg(test)]
pub mod test_helpers;
pub mod types;

use crate::state::ConsensusStateStore;
use crate::types::{AppState, ClientState};

declare_id!("F2G7Gtw2qVhG3uvvwr6w8h7n5ZzGy92cFQ3ZgkaX1AWe");
solana_allocator::custom_heap!();

pub use ics25_handler::{MembershipMsg, NonMembershipMsg};
pub use instructions::update_client::UpdateClientParams;
pub use types::ConsensusState;

#[derive(Accounts)]
#[instruction(client_id: String, latest_height: u64)]
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
        seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &latest_height.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
    #[account(
        init,
        payer = payer,
        space = 8 + AppState::INIT_SPACE,
        seeds = [AppState::SEED],
        bump
    )]
    pub app_state: Account<'info, AppState>,
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(msg: ics25_handler::MembershipMsg)]
pub struct VerifyMembership<'info> {
    pub client_state: Account<'info, ClientState>,
    #[account(
        seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &msg.height.to_le_bytes()],
        bump
    )]
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
#[instruction(msg: ics25_handler::NonMembershipMsg)]
pub struct VerifyNonMembership<'info> {
    pub client_state: Account<'info, ClientState>,
    #[account(
        seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &msg.height.to_le_bytes()],
        bump
    )]
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
#[instruction(client_id: String, new_height: u64)]
pub struct UpdateClient<'info> {
    #[account(
        mut,
        constraint = client_state.client_id == client_id,
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(
        seeds = [AppState::SEED],
        bump
    )]
    pub app_state: Account<'info, AppState>,

    /// CHECK: Validated by seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// CHECK: Instructions sysvar for role verification
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(
        init,
        payer = submitter,
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &new_height.to_le_bytes()],
        bump
    )]
    pub new_consensus_state_store: Account<'info, ConsensusStateStore>,

    #[account(mut)]
    pub submitter: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[program]
pub mod attestation_light_client {
    use super::*;
    use crate::instructions::update_client::UpdateClientParams;

    pub fn initialize(
        ctx: Context<Initialize>,
        client_id: String,
        latest_height: u64,
        attestor_addresses: Vec<[u8; 20]>,
        min_required_sigs: u8,
        timestamp: u64,
        access_manager: Pubkey,
    ) -> Result<()> {
        instructions::initialize::initialize(
            ctx,
            client_id,
            latest_height,
            attestor_addresses,
            min_required_sigs,
            timestamp,
            access_manager,
        )
    }

    /// Verifies the presence of a value at a given path in the counterparty chain state.
    /// Returns the timestamp of the consensus state at the proof height via return data.
    pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
        instructions::verify_membership::verify_membership(ctx, msg)
    }

    /// Verifies the absence of a value at a given path in the counterparty chain state.
    /// Returns the timestamp of the consensus state at the proof height via return data.
    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: NonMembershipMsg,
    ) -> Result<()> {
        instructions::verify_non_membership::verify_non_membership(ctx, msg)
    }

    pub fn update_client<'info>(
        ctx: Context<'_, '_, 'info, 'info, UpdateClient<'info>>,
        client_id: String,
        new_height: u64,
        params: UpdateClientParams,
    ) -> Result<()> {
        // Suppress unused warnings - these are used in account validation
        let _ = client_id;
        let _ = new_height;
        instructions::update_client::update_client(ctx, params)
    }
}
