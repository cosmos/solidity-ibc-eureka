use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg(test)]
pub mod test_utils;
pub mod types;

pub use errors::AccessManagerError;
pub use helpers::{
    handle_accept_access_manager_transfer, handle_cancel_access_manager_transfer,
    handle_propose_access_manager_transfer, require_admin, require_role,
    require_role_with_whitelist,
};
use instructions::*;
pub use state::AccessManagerTransferState;
pub use types::RoleData;

declare_id!("4fMih2CidrXPeRx77kj3QcuBZwREYtxEbXjURUgadoe1");

#[cfg(test)]
pub const PROGRAM_BINARY_NAME: &str = "access_manager";
#[cfg(test)]
pub const OTHER_AM_BINARY_NAME: &str = "test_access_manager";
#[cfg(test)]
pub const OTHER_AM_ID: Pubkey = solana_sdk::pubkey!("9dvkqiBj6G1fNZjNXEet88HSxy14dFBA3tCMaiSns9a3");

pub fn get_access_manager_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("access_manager_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/access_manager".to_string())
    })
}

#[program]
pub mod access_manager {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, admin: Pubkey) -> Result<()> {
        instructions::initialize(ctx, admin)
    }

    pub fn grant_role(ctx: Context<GrantRole>, role_id: u64, account: Pubkey) -> Result<()> {
        instructions::grant_role(ctx, role_id, account)
    }

    pub fn revoke_role(ctx: Context<RevokeRole>, role_id: u64, account: Pubkey) -> Result<()> {
        instructions::revoke_role(ctx, role_id, account)
    }

    pub fn renounce_role(ctx: Context<RenounceRole>, role_id: u64) -> Result<()> {
        instructions::renounce_role(ctx, role_id)
    }

    pub fn upgrade_program(ctx: Context<UpgradeProgram>, target_program: Pubkey) -> Result<()> {
        instructions::upgrade_program(ctx, target_program)
    }

    pub fn propose_upgrade_authority_transfer(
        ctx: Context<ProposeUpgradeAuthorityTransfer>,
        target_program: Pubkey,
        new_authority: Pubkey,
    ) -> Result<()> {
        instructions::propose_upgrade_authority_transfer(ctx, target_program, new_authority)
    }

    pub fn accept_upgrade_authority_transfer(
        ctx: Context<AcceptUpgradeAuthorityTransfer>,
        target_program: Pubkey,
    ) -> Result<()> {
        instructions::accept_upgrade_authority_transfer(ctx, target_program)
    }

    pub fn cancel_upgrade_authority_transfer(
        ctx: Context<CancelUpgradeAuthorityTransfer>,
        target_program: Pubkey,
    ) -> Result<()> {
        instructions::cancel_upgrade_authority_transfer(ctx, target_program)
    }

    pub fn set_whitelisted_programs(
        ctx: Context<SetWhitelistedPrograms>,
        whitelisted_programs: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::set_whitelisted_programs(ctx, whitelisted_programs)
    }

    pub fn claim_upgrade_authority(
        ctx: Context<ClaimUpgradeAuthority>,
        target_program: Pubkey,
    ) -> Result<()> {
        instructions::claim_upgrade_authority(ctx, target_program)
    }
}
