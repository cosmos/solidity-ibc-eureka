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

// TODO: Add access control
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
        let result = instructions::update_client::handler(ctx, update_msg)?;

        Ok(result)
    }

    /// Verify membership
    pub fn verify_membership(
        ctx: Context<VerifyMembership>,
        msg: ics25_handler::MembershipMsg,
    ) -> Result<()> {
        instructions::verify_membership::handler(ctx, msg)
    }

    // TODO: CRITICAL - Add signature verification before calling handler
    // The Solidity implementation verifies attestor signatures in verifyNonMembership()
    // by calling _verifySignaturesThreshold() which:
    // 1. Computes sha256 digest of proof.attestationData
    // 2. Recovers signer from each ECDSA signature (65 bytes: r||s||v)
    // 3. Verifies each recovered signer is in the attestor set
    // 4. Checks for duplicate signers
    // 5. Ensures signature count meets minRequiredSigs threshold
    // Currently, this implementation does NOT verify signatures at all!
    // See: contracts/light-clients/attestation/AttestationLightClient.sol:165-201
    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: ics25_handler::NonMembershipMsg,
    ) -> Result<()> {
        instructions::verify_non_membership::handler(ctx, msg)
    }

    // TODO: Add getter view functions for querying state
    // The Solidity implementation provides several view functions that are missing here:
    // 1. get_client_state() -> returns serialized ClientState
    //    See: AttestationLightClient.sol:74-76
    // 2. get_attestation_set() -> returns (Vec<[u8; 20]>, u8) with attestor addresses and min_required_sigs
    //    See: AttestationLightClient.sol:79-81
    // 3. get_consensus_timestamp(height: u64) -> returns u64 timestamp
    //    See: AttestationLightClient.sol:84-86
    // These could be implemented as read-only instructions or as part of the client state account

    // TODO: Implement access control mechanism
    // The Solidity implementation uses OpenZeppelin's AccessControl with PROOF_SUBMITTER_ROLE.
    // Key features:
    // 1. PROOF_SUBMITTER_ROLE constant (keccak256("PROOF_SUBMITTER_ROLE"))
    // 2. If address(0) has the role, anyone can submit proofs
    // 3. Otherwise, only addresses with the role can call verify_membership, verify_non_membership, update_client
    // 4. Role management via DEFAULT_ADMIN_ROLE
    // See: AttestationLightClient.sol:25, 37-70, 257-262
    // Consider using Solana's account-based permissions or implementing a role-based system in state

    // TODO: Implement misbehaviour instruction
    // The Solidity implementation has a misbehaviour() function (currently just returns FeatureNotSupported)
    // This is a placeholder for future misbehavior handling beyond the automatic detection in update_client.
    // See: AttestationLightClient.sol:204-207
}
