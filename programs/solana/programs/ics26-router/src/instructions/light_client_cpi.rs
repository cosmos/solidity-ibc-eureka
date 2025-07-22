use crate::errors::RouterError;
use crate::state::{Client, ClientType};
use anchor_lang::prelude::*;
use ics07_tendermint::cpi::accounts::{VerifyMembership, VerifyNonMembership};
use ics07_tendermint::cpi::{verify_membership, verify_non_membership};
use ics07_tendermint::MembershipMsg;

/// Message structure for light client non-membership verification
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct NonMembershipMsg {
    pub height: u64,
    pub delay_time_period: u64,
    pub delay_block_period: u64,
    pub proof: Vec<u8>,
    pub path: Vec<Vec<u8>>,
}

/// Accounts needed for light client verification via CPI
#[derive(Accounts)]
pub struct LightClientVerification<'info> {
    /// CHECK: Light client program, validated against client registry
    pub light_client_program: AccountInfo<'info>,

    /// CHECK: Client state account, owned by light client program
    pub client_state: AccountInfo<'info>,

    /// CHECK: Consensus state account, owned by light client program
    pub consensus_state: AccountInfo<'info>,
}

pub fn verify_membership_cpi(
    client: &Client,
    light_client_accounts: &LightClientVerification,
    membership_msg: MembershipMsg,
) -> Result<u64> {
    require!(
        light_client_accounts.light_client_program.key() == client.client_program_id,
        RouterError::InvalidLightClientProgram
    );

    require!(client.active, RouterError::ClientNotActive);

    match client.client_type {
        ClientType::ICS07Tendermint => {
            verify_tendermint_membership(light_client_accounts, membership_msg)
        }
    }
}

pub fn verify_non_membership_cpi(
    client: &Client,
    light_client_accounts: &LightClientVerification,
    non_membership_msg: NonMembershipMsg,
) -> Result<u64> {
    require!(
        light_client_accounts.light_client_program.key() == client.client_program_id,
        RouterError::InvalidLightClientProgram
    );

    require!(client.active, RouterError::ClientNotActive);

    match client.client_type {
        ClientType::ICS07Tendermint => {
            verify_tendermint_non_membership(light_client_accounts, non_membership_msg)
        }
    }
}

fn verify_tendermint_membership(
    light_client_accounts: &LightClientVerification,
    membership_msg: MembershipMsg,
) -> Result<u64> {
    verify_membership(
        CpiContext::new(
            light_client_accounts.light_client_program.to_account_info(),
            VerifyMembership {
                client_state: light_client_accounts.client_state.to_account_info(),
                consensus_state_at_height: light_client_accounts.consensus_state.to_account_info(),
            },
        ),
        membership_msg.clone(),
    )?;

    emit!(MembershipVerifiedEvent {
        client_type: "ICS07Tendermint".to_string(),
        height: membership_msg.height,
    });

    // Return the height as timestamp for now
    // In a real implementation, we might need to get this from the consensus state
    Ok(membership_msg.height)
}

fn verify_tendermint_non_membership(
    light_client_accounts: &LightClientVerification,
    non_membership_msg: NonMembershipMsg,
) -> Result<u64> {
    let membership_msg = MembershipMsg {
        height: non_membership_msg.height,
        delay_time_period: non_membership_msg.delay_time_period,
        delay_block_period: non_membership_msg.delay_block_period,
        proof: non_membership_msg.proof,
        path: non_membership_msg.path,
        value: vec![], // Empty value for non-membership
    };

    verify_non_membership(
        CpiContext::new(
            light_client_accounts.light_client_program.to_account_info(),
            VerifyNonMembership {
                client_state: light_client_accounts.client_state.to_account_info(),
                consensus_state_at_height: light_client_accounts.consensus_state.to_account_info(),
            },
        ),
        membership_msg.clone(),
    )?;

    emit!(NonMembershipVerifiedEvent {
        client_type: "ICS07Tendermint".to_string(),
        height: membership_msg.height,
    });

    // Return the height as timestamp
    Ok(membership_msg.height)
}

#[event]
pub struct MembershipVerifiedEvent {
    pub client_type: String,
    pub height: u64,
}

#[event]
pub struct NonMembershipVerifiedEvent {
    pub client_type: String,
    pub height: u64,
}
