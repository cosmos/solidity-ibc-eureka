use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke;
use crate::state::{ClientRegistry, ClientType};
use crate::errors::RouterError;
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
    client_registry: &ClientRegistry,
    light_client_accounts: &LightClientVerification,
    membership_msg: MembershipMsg,
) -> Result<u64> {
    require!(
        light_client_accounts.light_client_program.key() == client_registry.client_program_id,
        RouterError::InvalidLightClientProgram
    );

    require!(
        client_registry.active,
        RouterError::ClientNotActive
    );

    match client_registry.client_type {
        ClientType::ICS07Tendermint => {
            verify_tendermint_membership(light_client_accounts, membership_msg)
        }
    }
}

pub fn verify_non_membership_cpi(
    client_registry: &ClientRegistry,
    light_client_accounts: &LightClientVerification,
    non_membership_msg: NonMembershipMsg,
) -> Result<u64> {
    require!(
        light_client_accounts.light_client_program.key() == client_registry.client_program_id,
        RouterError::InvalidLightClientProgram
    );

    require!(
        client_registry.active,
        RouterError::ClientNotActive
    );

    match client_registry.client_type {
        ClientType::ICS07Tendermint => {
            verify_tendermint_non_membership(light_client_accounts, non_membership_msg)
        }
    }
}

fn verify_tendermint_membership(
    light_client_accounts: &LightClientVerification,
    membership_msg: MembershipMsg,
) -> Result<u64> {
    // TODO: use build.rs to compute
    // Define the instruction discriminator for verify_membership
    // This is the 8-byte discriminator that Anchor generates for the instruction
    const VERIFY_MEMBERSHIP_IX_DISCM: [u8; 8] = [117, 157, 187, 21, 220, 192, 82, 200];

    let mut ix_data = Vec::new();
    ix_data.extend_from_slice(&VERIFY_MEMBERSHIP_IX_DISCM);
    membership_msg.serialize(&mut ix_data)?;

    let ix = Instruction {
        program_id: light_client_accounts.light_client_program.key(),
        accounts: vec![
            AccountMeta::new_readonly(light_client_accounts.client_state.key(), false),
            AccountMeta::new_readonly(light_client_accounts.consensus_state.key(), false),
        ],
        data: ix_data,
    };

    let account_infos = vec![
        light_client_accounts.client_state.to_account_info(),
        light_client_accounts.consensus_state.to_account_info(),
        light_client_accounts.light_client_program.to_account_info(),
    ];

    invoke(&ix, &account_infos)?;

    msg!("Verified membership proof via CPI to ICS07 Tendermint");

    // Return the height as timestamp for now
    // In a real implementation, we might need to get this from the consensus state
    Ok(membership_msg.height)
}

fn verify_tendermint_non_membership(
    light_client_accounts: &LightClientVerification,
    non_membership_msg: NonMembershipMsg,
) -> Result<u64> {
    // TODO: use build.rs to compute
    // Define the instruction discriminator for verify_non_membership
    // This is the 8-byte discriminator that Anchor generates for the instruction
    const VERIFY_NON_MEMBERSHIP_IX_DISCM: [u8; 8] = [122, 152, 236, 247, 57, 132, 159, 5];

    let membership_msg = MembershipMsg {
        height: non_membership_msg.height,
        delay_time_period: non_membership_msg.delay_time_period,
        delay_block_period: non_membership_msg.delay_block_period,
        proof: non_membership_msg.proof,
        path: non_membership_msg.path,
        value: vec![], // Empty value for non-membership
    };

    let mut ix_data = Vec::new();
    ix_data.extend_from_slice(&VERIFY_NON_MEMBERSHIP_IX_DISCM);
    membership_msg.serialize(&mut ix_data)?;

    let ix = Instruction {
        program_id: light_client_accounts.light_client_program.key(),
        accounts: vec![
            AccountMeta::new_readonly(light_client_accounts.client_state.key(), false),
            AccountMeta::new_readonly(light_client_accounts.consensus_state.key(), false),
        ],
        data: ix_data,
    };

    let account_infos = vec![
        light_client_accounts.client_state.to_account_info(),
        light_client_accounts.consensus_state.to_account_info(),
        light_client_accounts.light_client_program.to_account_info(),
    ];

    invoke(&ix, &account_infos)?;

    msg!("Verified non-membership proof via CPI to ICS07 Tendermint");

    // Return the height as timestamp
    Ok(membership_msg.height)
}

