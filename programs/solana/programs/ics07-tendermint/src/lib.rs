use anchor_lang::prelude::*;

pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg(test)]
pub mod test_helpers;
pub mod types;

use crate::state::{ConsensusStateStore, HeaderChunk, HeaderMetadata};

declare_id!("HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD");

pub use types::{
    ClientState, ConsensusState, IbcHeight, MisbehaviourMsg, UpdateClientMsg, UpdateResult,
};

pub use ics25_handler::MembershipMsg;

#[derive(Accounts)]
#[instruction(chain_id: String, latest_height: u64, client_state: ClientState)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ClientState::INIT_SPACE,
        seeds = [b"client", chain_id.as_bytes()],
        bump
    )]
    pub client_state: Account<'info, ClientState>,
    #[account(
        init,
        payer = payer,
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [b"consensus_state", client_state.key().as_ref(), &latest_height.to_le_bytes()],
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
    pub client_state: Account<'info, ClientState>,
    /// Trusted consensus state at the height specified in the header
    /// We use `UncheckedAccount` here because the trusted height is extracted from the header,
    /// which can only be deserialized inside the instruction handler. Since Anchor's account
    /// validation happens before the instruction code runs, we cannot use the standard
    /// #[account(seeds = ...)] constraint. Instead, we manually validate the PDA derivation
    /// inside the instruction handler after extracting the trusted height from the header.
    /// CHECK: This account is validated in the instruction handler based on the trusted height from the header
    pub trusted_consensus_state: UncheckedAccount<'info>,
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

/// Context for uploading a chunk of header data
/// Chunks are stored by chain_id, height, and chunk index
#[derive(Accounts)]
#[instruction(chain_id: String, target_height: u64, chunk_index: u8, total_chunks: u8)]
pub struct UploadHeaderChunk<'info> {
    /// The header chunk account to create/update
    /// If there's an old chunk at this position, it will be overwritten
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + HeaderChunk::INIT_SPACE,
        seeds = [
            b"header_chunk",
            chain_id.as_bytes(),
            &target_height.to_le_bytes(),
            &[chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, HeaderChunk>,

    /// Header metadata for this height (created with first chunk)
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + HeaderMetadata::INIT_SPACE,
        seeds = [
            b"header_metadata",
            chain_id.as_bytes(),
            &target_height.to_le_bytes()
        ],
        bump
    )]
    pub metadata: Account<'info, HeaderMetadata>,

    /// Client state to verify this is a valid client
    #[account(
        constraint = client_state.chain_id == chain_id,
        constraint = target_height > client_state.latest_height.revision_height
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

/// Context for assembling chunks and updating the client
/// This will automatically clean up any old chunks at the same height
#[derive(Accounts)]
pub struct AssembleAndUpdateClient<'info> {
    #[account(
        mut,
        constraint = client_state.chain_id == metadata.chain_id.as_str(),
        constraint = metadata.target_height > client_state.latest_height.revision_height
    )]
    pub client_state: Account<'info, ClientState>,

    /// Header metadata for this height
    #[account(
        mut,
        close = payer  // Close metadata after successful update
    )]
    pub metadata: Account<'info, HeaderMetadata>,

    /// Trusted consensus state (will be validated after header assembly)
    /// CHECK: Validated in instruction handler after header reassembly
    pub trusted_consensus_state: UncheckedAccount<'info>,

    /// New consensus state store
    /// CHECK: Validated in instruction handler
    pub new_consensus_state_store: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
    // Remaining accounts are the chunk accounts in order
    // They will be validated and closed in the instruction handler
}

/// Context for cleaning up incomplete header uploads at lower heights
#[derive(Accounts)]
#[instruction(chain_id: String, cleanup_height: u64)]
pub struct CleanupIncompleteUpload<'info> {
    /// Client state to verify this is a valid client
    #[account(
        constraint = client_state.chain_id == chain_id,
        constraint = cleanup_height < client_state.latest_height.revision_height
    )]
    pub client_state: Account<'info, ClientState>,

    /// Header metadata for the height to clean up
    #[account(
        mut,
        seeds = [
            b"header_metadata",
            chain_id.as_bytes(),
            &cleanup_height.to_le_bytes()
        ],
        bump,
        close = payer  // Close metadata to reclaim rent
    )]
    pub metadata: Account<'info, HeaderMetadata>,

    #[account(mut)]
    pub payer: Signer<'info>,
    // Remaining accounts are the chunk accounts to close
}

#[program]
pub mod ics07_tendermint {
    use super::*;
    use crate::types::{ClientState, ConsensusState, MisbehaviourMsg, UpdateClientMsg};

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

    /// Upload a chunk of header data for multi-transaction updates
    /// Automatically overwrites any existing chunk at the same position
    pub fn upload_header_chunk(
        ctx: Context<UploadHeaderChunk>,
        chain_id: String,
        target_height: u64,
        chunk_index: u8,
        total_chunks: u8,
        chunk_data: Vec<u8>,
        chunk_hash: [u8; 32],
        header_commitment: [u8; 32],
    ) -> Result<()> {
        instructions::upload_header_chunk::upload_header_chunk(
            ctx,
            chain_id,
            target_height,
            chunk_index,
            total_chunks,
            chunk_data,
            chunk_hash,
            header_commitment,
        )
    }

    /// Assemble chunks and update the client
    /// Automatically cleans up all chunks after successful update
    pub fn assemble_and_update_client(
        ctx: Context<AssembleAndUpdateClient>,
    ) -> Result<UpdateResult> {
        instructions::assemble_and_update_client::assemble_and_update_client(ctx)
    }

    /// Clean up incomplete header uploads at lower heights
    /// This can be called to reclaim rent from failed or abandoned uploads
    pub fn cleanup_incomplete_upload(
        ctx: Context<CleanupIncompleteUpload>,
        chain_id: String,
        cleanup_height: u64,
    ) -> Result<()> {
        instructions::cleanup_incomplete_upload::cleanup_incomplete_upload(
            ctx,
            chain_id,
            cleanup_height,
        )
    }
}
