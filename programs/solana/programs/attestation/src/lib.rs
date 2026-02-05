use anchor_lang::prelude::*;

pub mod abi_decode;
pub mod crypto;
pub mod error;
pub mod events;
pub mod instructions;
pub mod proof;
pub mod state;
#[cfg(test)]
pub mod test_helpers;
pub mod types;
pub mod verification;

use instructions::*;

declare_id!("F2G7Gtw2qVhG3uvvwr6w8h7n5ZzGy92cFQ3ZgkaX1AWe");
solana_allocator::custom_heap!();

pub use crypto::ETH_ADDRESS_LEN;
pub use ics25_handler::{MembershipMsg, NonMembershipMsg};
pub use instructions::update_client::UpdateClientParams;
pub use types::ConsensusState;

#[program]
pub mod attestation {
    use super::*;
    use crate::instructions::update_client::UpdateClientParams;

    pub fn initialize(
        ctx: Context<Initialize>,
        client_id: String,
        latest_height: u64,
        attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
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

    pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
        instructions::verify_membership::verify_membership(ctx, msg)
    }

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
        let _ = client_id;
        instructions::update_client::update_client(ctx, new_height, params)
    }
}
