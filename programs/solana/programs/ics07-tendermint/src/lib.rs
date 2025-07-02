#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;

declare_id!("8wQAC7oWLTxExhR49jYAzXZB39mu7WVVvkWJGgAMMjpV");

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClientState {
    pub chain_id: String,
    pub trust_level_numerator: u64,
    pub trust_level_denominator: u64,
    pub trusting_period: u64,
    pub unbonding_period: u64,
    pub max_clock_drift: u64,
    pub frozen_height: u64,
    pub latest_height: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ConsensusState {
    pub timestamp: u64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateClientMsg {
    pub client_message: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MembershipMsg {
    pub height: u64,
    pub delay_time_period: u64,
    pub delay_block_period: u64,
    pub proof: Vec<u8>,
    pub path: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MisbehaviourMsg {
    pub client_id: String,
    pub header_1: Vec<u8>,
    pub header_2: Vec<u8>,
}

#[account]
pub struct ClientData {
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
    pub frozen: bool,
}

#[derive(Accounts)]
#[instruction(chain_id: String)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + 1000,
        seeds = [b"client", chain_id.as_bytes()],
        bump
    )]
    pub client_data: Account<'info, ClientData>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateClient<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
}

#[derive(Accounts)]
pub struct VerifyMembership<'info> {
    pub client_data: Account<'info, ClientData>,
}

#[derive(Accounts)]
pub struct VerifyNonMembership<'info> {
    pub client_data: Account<'info, ClientData>,
}

#[derive(Accounts)]
pub struct SubmitMisbehaviour<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
}

#[program]
pub mod ics07_tendermint {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> Result<()> {
        let client_data = &mut ctx.accounts.client_data;
        client_data.client_state = client_state;
        client_data.consensus_state = consensus_state;
        client_data.frozen = false;
        Ok(())
    }

    pub fn update_client(ctx: Context<UpdateClient>, _msg: UpdateClientMsg) -> Result<()> {
        let _client_data = &mut ctx.accounts.client_data;
        Ok(())
    }

    pub fn verify_membership(ctx: Context<VerifyMembership>, _msg: MembershipMsg) -> Result<()> {
        let _client_data = &ctx.accounts.client_data;
        Ok(())
    }

    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        _msg: MembershipMsg,
    ) -> Result<()> {
        let _client_data = &ctx.accounts.client_data;
        Ok(())
    }

    pub fn submit_misbehaviour(
        ctx: Context<SubmitMisbehaviour>,
        _msg: MisbehaviourMsg,
    ) -> Result<()> {
        let _client_data = &mut ctx.accounts.client_data;
        Ok(())
    }
}
