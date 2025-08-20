use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;
use ics25_handler::MembershipMsg;

declare_id!("4nFbkWTbUxKwXqKHzLdxkUfYZ9MrVkzJp7nXt8GY7JKp");

#[program]
pub mod mock_light_client {
    use super::*;

    pub fn verify_membership(_ctx: Context<VerifyMembership>, _msg: MembershipMsg) -> Result<()> {
        msg!("Mock light client: verify_membership always returns success");

        // Return the height as bytes for membership verification
        let height_bytes = _msg.height.to_le_bytes();
        set_return_data(&height_bytes);

        Ok(())
    }

    pub fn verify_non_membership(
        _ctx: Context<VerifyNonMembership>,
        _msg: MembershipMsg,
    ) -> Result<()> {
        msg!("Mock light client: verify_non_membership always returns success");

        // For non-membership (timeout), return a timestamp value
        // Using 2000 as a mock timestamp (greater than typical test timeout values)
        let timestamp: u64 = 2000;
        let timestamp_bytes = timestamp.to_le_bytes();
        set_return_data(&timestamp_bytes);

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
