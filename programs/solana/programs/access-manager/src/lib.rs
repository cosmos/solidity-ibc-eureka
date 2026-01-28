#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
pub mod test_utils;
pub mod types;

pub use errors::AccessManagerError;
pub use helpers::require_role;
use instructions::*;
pub use types::RoleData;

declare_id!("4fMih2CidrXPeRx77kj3QcuBZwREYtxEbXjURUgadoe1");

/// Programs whitelisted to call certain instructions via CPI
pub const WHITELISTED_CPI_PROGRAMS: &[Pubkey] = &[];

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
}
