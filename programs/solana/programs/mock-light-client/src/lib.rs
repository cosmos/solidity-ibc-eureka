use anchor_lang::prelude::*;
use solana_light_client_interface::MembershipMsg;

declare_id!("4nFbkWTbUxKwXqKHzLdxkUfYZ9MrVkzJp7nXt8GY7JKp");

#[program]
pub mod mock_light_client {
    use super::*;

    pub fn verify_membership(_ctx: Context<VerifyMembership>, _msg: MembershipMsg) -> Result<()> {
        msg!("Mock light client: verify_membership always returns success");
        Ok(())
    }

    pub fn verify_non_membership(
        _ctx: Context<VerifyNonMembership>,
        _msg: MembershipMsg,
    ) -> Result<()> {
        msg!("Mock light client: verify_non_membership always returns success");
        Ok(())
    }
}

#[derive(Accounts)]
pub struct VerifyMembership<'info> {
    /// CHECK: Mock client state - not actually used
    pub client_state: AccountInfo<'info>,
    /// CHECK: Mock consensus state - not actually used
    pub consensus_state: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct VerifyNonMembership<'info> {
    /// CHECK: Mock client state - not actually used
    pub client_state: AccountInfo<'info>,
    /// CHECK: Mock consensus state - not actually used
    pub consensus_state: AccountInfo<'info>,
}
