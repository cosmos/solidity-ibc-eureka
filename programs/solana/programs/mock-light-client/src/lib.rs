use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;
use ics25_handler::MembershipMsg;
use solana_ibc_types::ics07::UpdateClientMsg;

declare_id!("CSLS3A9jS7JAD8aUe3LRXMYZ1U8Lvxn9usGygVrA2arZ");

#[program]
pub mod mock_light_client {
    use super::*;

    pub fn initialize(
        _ctx: Context<Initialize>,
        _chain_id: String,
        _latest_height: u64,
        _client_state: Vec<u8>,
        _consensus_state: Vec<u8>,
    ) -> Result<()> {
        msg!("Mock light client: initialize always returns success");
        Ok(())
    }

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

    pub fn update_client(_ctx: Context<UpdateClient>, _msg: UpdateClientMsg) -> Result<u8> {
        msg!("Mock light client: update_client always returns success");

        // For mock client, always return Update (not NoOp)
        // This matches the UpdateResult enum: Update = 0, NoOp = 1
        Ok(0)
    }
}

#[derive(Accounts)]
#[instruction(chain_id: String, latest_height: u64, client_state: Vec<u8>)]
pub struct Initialize<'info> {
    /// CHECK: Mock client state - account will be created but not used
    #[account(
        init,
        payer = payer,
        space = 8 + 1024, // Mock space allocation
        seeds = [b"client", chain_id.as_bytes()],
        bump
    )]
    pub client_state: AccountInfo<'info>,
    /// CHECK: Mock consensus state - account will be created but not used
    #[account(
        init,
        payer = payer,
        space = 8 + 512, // Mock space allocation
        seeds = [b"consensus_state", client_state.key().as_ref(), &latest_height.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: AccountInfo<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
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

#[derive(Accounts)]
pub struct UpdateClient<'info> {
    /// CHECK: Mock client state - not actually used
    pub client_state: AccountInfo<'info>,
    /// CHECK: Mock trusted consensus state - not actually used
    pub trusted_consensus_state: AccountInfo<'info>,
    /// CHECK: Mock new consensus state - not actually used  
    pub new_consensus_state: AccountInfo<'info>,
    /// CHECK: Mock payer - not actually used
    pub payer: AccountInfo<'info>,
    /// CHECK: Mock system program - not actually used
    pub system_program: AccountInfo<'info>,
}
