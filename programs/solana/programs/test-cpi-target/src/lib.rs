use anchor_lang::prelude::*;

declare_id!("GHB99UGVmKFeNrtSLsuzL2QhZZgaqcASvTjotQd2dZzu");

pub mod instructions;
#[cfg(test)]
mod test_utils;

use instructions::*;

/// Test-only program that wraps each `cpi.rs` validation function as an
/// instruction so they can be exercised under a real BPF runtime via
/// `ProgramTest`.
#[program]
pub mod test_cpi_target {
    use super::*;

    pub fn check_is_cpi(ctx: Context<CheckIsCpi>) -> Result<()> {
        instructions::check_is_cpi::check_is_cpi(ctx)
    }

    pub fn check_reject_direct_calls(ctx: Context<CheckRejectDirectCalls>) -> Result<()> {
        instructions::check_reject_direct_calls::check_reject_direct_calls(ctx)
    }

    pub fn check_reject_nested_cpi(ctx: Context<CheckRejectNestedCpi>) -> Result<()> {
        instructions::check_reject_nested_cpi::check_reject_nested_cpi(ctx)
    }

    pub fn check_validate_cpi_caller(
        ctx: Context<CheckValidateCpiCaller>,
        authorized_program: Pubkey,
    ) -> Result<()> {
        instructions::check_validate_cpi_caller::check_validate_cpi_caller(ctx, authorized_program)
    }

    pub fn check_reject_cpi(ctx: Context<CheckRejectCpi>) -> Result<()> {
        instructions::check_reject_cpi::check_reject_cpi(ctx)
    }

    pub fn check_direct_or_whitelisted(
        ctx: Context<CheckDirectOrWhitelisted>,
        whitelisted_programs: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::check_direct_or_whitelisted::check_direct_or_whitelisted(
            ctx,
            whitelisted_programs,
        )
    }

    pub fn check_require_admin(ctx: Context<CheckRequireAdmin>) -> Result<()> {
        instructions::check_require_admin::check_require_admin(ctx)
    }

    pub fn check_require_role(ctx: Context<CheckRequireRole>, role_id: u64) -> Result<()> {
        instructions::check_require_role::check_require_role(ctx, role_id)
    }

    pub fn check_require_role_with_whitelist(
        ctx: Context<CheckRequireRoleWithWhitelist>,
        role_id: u64,
    ) -> Result<()> {
        instructions::check_require_role_with_whitelist::check_require_role_with_whitelist(
            ctx, role_id,
        )
    }

    pub fn proxy_cpi<'info>(
        ctx: Context<'_, '_, '_, 'info, ProxyCpi<'info>>,
        instruction_data: Vec<u8>,
        account_metas: Vec<CpiAccountMeta>,
    ) -> Result<()> {
        instructions::proxy_cpi::proxy_cpi(ctx, instruction_data, account_metas)
    }
}
