use crate::errors::RouterError;
use crate::state::Client;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke;
use solana_light_client_interface::{discriminators, MembershipMsg};

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

    let mut ix_data = Vec::new();
    ix_data.extend_from_slice(&discriminators::VERIFY_MEMBERSHIP);
    membership_msg.serialize(&mut ix_data)?;

    // Build the instruction with standard account layout
    // All light clients must accept: [client_state, consensus_state]
    let ix = Instruction {
        program_id: client.client_program_id,
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

    Ok(membership_msg.height)
}

pub fn verify_non_membership_cpi(
    client: &Client,
    light_client_accounts: &LightClientVerification,
    non_membership_msg: MembershipMsg,
) -> Result<u64> {
    require!(
        light_client_accounts.light_client_program.key() == client.client_program_id,
        RouterError::InvalidLightClientProgram
    );

    require!(client.active, RouterError::ClientNotActive);

    let membership_msg = MembershipMsg {
        height: non_membership_msg.height,
        delay_time_period: non_membership_msg.delay_time_period,
        delay_block_period: non_membership_msg.delay_block_period,
        proof: non_membership_msg.proof,
        path: non_membership_msg.path,
        value: vec![], // Empty value for non-membership
    };

    let mut ix_data = Vec::new();
    ix_data.extend_from_slice(&discriminators::VERIFY_NON_MEMBERSHIP);
    membership_msg.serialize(&mut ix_data)?;

    let ix = Instruction {
        program_id: client.client_program_id,
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

    Ok(membership_msg.height)
}
