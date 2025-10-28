use anchor_lang::prelude::*;

pub mod constants;
pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg(test)]
pub mod test_helpers;
pub mod types;

use crate::state::{ConsensusStateStore, HeaderChunk};

declare_id!("HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD");

pub use types::{
    ClientState, ConsensusState, IbcHeight, MisbehaviourMsg, UpdateResult, UploadChunkParams,
};

pub use ics25_handler::MembershipMsg;

#[derive(Accounts)]
#[instruction(chain_id: String, latest_height: u64, client_state: ClientState)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ClientState::INIT_SPACE,
        seeds = [ClientState::SEED, chain_id.as_bytes()],
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

#[derive(Accounts)]
#[instruction(params: types::UploadChunkParams)]
pub struct UploadHeaderChunk<'info> {
    /// The header chunk account to create (fails if already exists)
    #[account(
        init,
        payer = submitter,
        space = 8 + HeaderChunk::INIT_SPACE,
        seeds = [
            HeaderChunk::SEED,
            submitter.key().as_ref(),
            params.chain_id.as_bytes(),
            &params.target_height.to_le_bytes(),
            &[params.chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, HeaderChunk>,

    /// Client state to verify this is a valid client
    #[account(
        constraint = client_state.chain_id == params.chain_id,
    )]
    pub client_state: Account<'info, ClientState>,

    /// The submitter who pays for and owns these accounts
    #[account(mut)]
    pub submitter: Signer<'info>,
    pub system_program: Program<'info, System>,
}

/// Context for assembling chunks and updating the client
/// This will automatically clean up any old chunks at the same height
#[derive(Accounts)]
#[instruction(chain_id: String, target_height: u64)]
pub struct AssembleAndUpdateClient<'info> {
    #[account(
        mut,
        constraint = client_state.chain_id == chain_id.as_str(),
    )]
    pub client_state: Account<'info, ClientState>,

    /// Trusted consensus state (will be validated after header assembly)
    /// CHECK: Validated in instruction handler after header reassembly
    pub trusted_consensus_state: UncheckedAccount<'info>,

    /// New consensus state store
    /// CHECK: Validated in instruction handler
    pub new_consensus_state_store: UncheckedAccount<'info>,

    /// The original submitter who paid for the chunks (receives rent back)
    /// CHECK: Must be the same submitter who created the chunks
    #[account(mut)]
    pub submitter: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
    // Remaining accounts are the chunk accounts in order
    // They will be validated and closed in the instruction handler
}

/// Context for cleaning up incomplete header uploads
/// This can be called to reclaim rent from failed or abandoned chunk uploads
#[derive(Accounts)]
#[instruction(chain_id: String, cleanup_height: u64, submitter: Pubkey)]
pub struct CleanupIncompleteUpload<'info> {
    /// Client state to verify this is a valid client
    #[account(
        constraint = client_state.chain_id == chain_id,
    )]
    pub client_state: Account<'info, ClientState>,

    /// The original submitter who gets their rent back
    /// Must be the signer to prove they own the upload
    #[account(
        mut,
        constraint = submitter_account.key() == submitter
    )]
    pub submitter_account: Signer<'info>,
    // Remaining accounts are the chunk accounts to close
}

#[program]
pub mod ics07_tendermint {
    use super::*;
    use crate::types::{ClientState, ConsensusState, MisbehaviourMsg, UploadChunkParams};

    pub fn initialize(
        ctx: Context<Initialize>,
        chain_id: String,
        latest_height: u64,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> Result<()> {
        // NOTE: chain_id is used in the #[instruction] attribute for account validation
        // but the actual handler doesn't need it as it's embedded in client_state
        assert_eq!(client_state.chain_id, chain_id);
        assert_eq!(client_state.latest_height.revision_height, latest_height);

        instructions::initialize::initialize(ctx, client_state, consensus_state)
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

    /// Upload a chunk of header data for multi-transaction updates
    /// Fails if a chunk already exists at this position (no overwrites allowed)
    pub fn upload_header_chunk(
        ctx: Context<UploadHeaderChunk>,
        params: UploadChunkParams,
    ) -> Result<()> {
        instructions::upload_header_chunk::upload_header_chunk(ctx, params)
    }

    /// Assemble chunks and update the client
    /// Automatically cleans up all chunks after successful update
    pub fn assemble_and_update_client(
        ctx: Context<AssembleAndUpdateClient>,
        chain_id: String,
        target_height: u64,
    ) -> Result<UpdateResult> {
        instructions::assemble_and_update_client::assemble_and_update_client(
            ctx,
            chain_id,
            target_height,
        )
    }

    /// Clean up incomplete header uploads at lower heights
    /// This can be called to reclaim rent from failed or abandoned uploads
    pub fn cleanup_incomplete_upload(
        ctx: Context<CleanupIncompleteUpload>,
        chain_id: String,
        cleanup_height: u64,
        submitter: Pubkey,
    ) -> Result<()> {
        instructions::cleanup_incomplete_upload::cleanup_incomplete_upload(
            ctx,
            chain_id,
            cleanup_height,
            submitter,
        )
    }
}
