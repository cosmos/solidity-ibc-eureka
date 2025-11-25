use anchor_lang::prelude::*;

pub mod conversions;
pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg(test)]
pub mod test_helpers;
pub mod types;

use crate::state::{ConsensusStateStore, HeaderChunk, MisbehaviourChunk};

declare_id!("HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD");
solana_allocator::custom_heap!();

pub use types::{
    ClientState, ConsensusState, IbcHeight, UpdateResult, UploadChunkParams,
    UploadMisbehaviourChunkParams,
};

pub use ics25_handler::{MembershipMsg, NonMembershipMsg};

#[derive(Accounts)]
#[instruction(chain_id: String, latest_height: u64, client_state: ClientState, consensus_state: ConsensusState)]
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
#[instruction(chain_id: String)]
pub struct SetAccessManager<'info> {
    #[account(
        mut,
        seeds = [ClientState::SEED, chain_id.as_bytes()],
        bump,
        constraint = client_state.chain_id == chain_id.as_str()
    )]
    pub client_state: Account<'info, ClientState>,

    /// CHECK: Validated via seeds constraint using the stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = client_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    pub admin: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
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
#[instruction(chain_id: String, target_height: u64, chunk_count: u8)]
pub struct AssembleAndUpdateClient<'info> {
    #[account(
        mut,
        constraint = client_state.chain_id == chain_id.as_str(),
    )]
    pub client_state: Account<'info, ClientState>,

    /// Global access control account (owned by access-manager program)
    /// CHECK: Validated by seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = client_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Trusted consensus state at the height embedded in the header
    /// CHECK: Must already exist. Unchecked because PDA seeds require runtime header data.
    pub trusted_consensus_state: UncheckedAccount<'info>,

    /// New consensus state store
    /// CHECK: Validated in instruction handler. Unchecked because may not exist yet and PDA seeds require runtime height.
    pub new_consensus_state_store: UncheckedAccount<'info>,

    /// The submitter who uploaded the chunks
    #[account(mut)]
    pub submitter: Signer<'info>,

    pub system_program: Program<'info, System>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
    // Remaining accounts are the chunk accounts in order
    // They will be validated and closed in the instruction handler
}

/// Context for cleaning up incomplete header uploads or signatures
#[derive(Accounts)]
pub struct CleanupIncompleteUpload<'info> {
    /// The original submitter who gets their rent back
    /// Must be the signer to prove they own the upload
    #[account(mut)]
    pub submitter: Signer<'info>,
    // Remaining accounts are the chunk and signature verification accounts to close
}

#[derive(Accounts)]
#[instruction(params: types::UploadMisbehaviourChunkParams)]
pub struct UploadMisbehaviourChunk<'info> {
    #[account(
        init,
        payer = submitter,
        space = 8 + MisbehaviourChunk::INIT_SPACE,
        seeds = [
            MisbehaviourChunk::SEED,
            submitter.key().as_ref(),
            params.client_id.as_bytes(),
            &[params.chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, MisbehaviourChunk>,

    #[account(
        constraint = client_state.chain_id == params.client_id,
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(mut)]
    pub submitter: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(client_id: String)]
pub struct AssembleAndSubmitMisbehaviour<'info> {
    #[account(
        mut,
        constraint = client_state.chain_id == client_id.as_str(),
    )]
    pub client_state: Account<'info, ClientState>,

    /// Global access control account (owned by access-manager program)
    /// CHECK: Validated by seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = client_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    pub trusted_consensus_state_1: Account<'info, ConsensusStateStore>,

    pub trusted_consensus_state_2: Account<'info, ConsensusStateStore>,

    #[account(mut)]
    pub submitter: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
    // Remaining accounts are the chunk accounts in order
}

#[derive(Accounts)]
#[instruction(client_id: String, submitter: Pubkey)]
pub struct CleanupIncompleteMisbehaviour<'info> {
    #[account(
        constraint = client_state.chain_id == client_id,
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(
        mut,
        constraint = submitter_account.key() == submitter
    )]
    pub submitter_account: Signer<'info>,
    // Remaining accounts are the chunk accounts to close
}

#[derive(Accounts)]
#[instruction(signature: solana_ibc_types::ics07::SignatureData)]
pub struct PreVerifySignature<'info> {
    /// CHECK: Sysvar instructions account
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<crate::state::SignatureVerification>(),
        seeds = [
            crate::state::SignatureVerification::SEED,
            &signature.signature_hash
        ],
        bump
    )]
    pub signature_verification: Account<'info, crate::state::SignatureVerification>,

    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[program]
pub mod ics07_tendermint {
    use super::*;
    use crate::types::{
        ClientState, ConsensusState, UploadChunkParams, UploadMisbehaviourChunkParams,
    };

    pub fn initialize(
        ctx: Context<Initialize>,
        chain_id: String,
        latest_height: u64,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> Result<()> {
        instructions::initialize::initialize(
            ctx,
            chain_id,
            latest_height,
            client_state,
            consensus_state,
        )
    }

    pub fn set_access_manager(
        ctx: Context<SetAccessManager>,
        chain_id: String,
        new_access_manager: Pubkey,
    ) -> Result<()> {
        instructions::set_access_manager::set_access_manager(ctx, chain_id, new_access_manager)
    }

    pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
        instructions::verify_membership::verify_membership(ctx, msg)
    }

    /// Verifies the absence of a value at a given path in the counterparty chain state.
    /// Returns the timestamp of the consensus state at the proof height in unix seconds.
    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: NonMembershipMsg,
    ) -> Result<()> {
        instructions::verify_non_membership::verify_non_membership(ctx, msg)
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
    pub fn assemble_and_update_client<'info>(
        ctx: Context<'_, '_, 'info, 'info, AssembleAndUpdateClient<'info>>,
        chain_id: String,
        target_height: u64,
        chunk_count: u8,
    ) -> Result<UpdateResult> {
        instructions::assemble_and_update_client::assemble_and_update_client(
            ctx,
            chain_id,
            target_height,
            chunk_count,
        )
    }

    /// Clean up incomplete header uploads
    /// This can be called to reclaim rent from failed or abandoned uploads
    /// Closes both `HeaderChunk` and `SignatureVerification` PDAs owned by the submitter
    pub fn cleanup_incomplete_upload(ctx: Context<CleanupIncompleteUpload>) -> Result<()> {
        instructions::cleanup_incomplete_upload::cleanup_incomplete_upload(ctx)
    }

    /// Upload a chunk of misbehaviour data for multi-transaction submission
    pub fn upload_misbehaviour_chunk(
        ctx: Context<UploadMisbehaviourChunk>,
        params: UploadMisbehaviourChunkParams,
    ) -> Result<()> {
        instructions::upload_misbehaviour_chunk::upload_misbehaviour_chunk(ctx, params)
    }

    /// Assemble chunks and submit misbehaviour
    /// Automatically freezes the client and cleans up all chunks
    pub fn assemble_and_submit_misbehaviour(
        ctx: Context<AssembleAndSubmitMisbehaviour>,
        client_id: String,
    ) -> Result<()> {
        instructions::assemble_and_submit_misbehaviour::assemble_and_submit_misbehaviour(
            ctx, client_id,
        )
    }

    /// Clean up incomplete misbehaviour uploads
    /// This can be called to reclaim rent from failed or abandoned misbehaviour submissions
    pub fn cleanup_incomplete_misbehaviour(
        ctx: Context<CleanupIncompleteMisbehaviour>,
        client_id: String,
        submitter: Pubkey,
    ) -> Result<()> {
        instructions::cleanup_incomplete_misbehaviour::cleanup_incomplete_misbehaviour(
            ctx, client_id, submitter,
        )
    }

    pub fn pre_verify_signature<'info>(
        ctx: Context<'_, '_, '_, 'info, PreVerifySignature<'info>>,
        signature: solana_ibc_types::ics07::SignatureData,
    ) -> Result<()> {
        instructions::pre_verify_signatures::pre_verify_signature(ctx, signature)
    }
}
