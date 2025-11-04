use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use crate::errors::RouterError;
use crate::state::Client;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::{get_return_data, invoke};
use ics25_handler::{discriminators, MembershipMsg, NonMembershipMsg};

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

/// Verifies membership (existence) of a value at a given path
pub fn verify_membership_cpi(
    client: &Client,
    light_client_accounts: &LightClientVerification,
    membership_msg: MembershipMsg,
) -> Result<()> {
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

    Ok(())
}

/// Verifies non-membership (absence) of a value at a given path
/// Returns the timestamp from the consensus state at the proof height
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

    let mut ix_data = Vec::new();
    ix_data.extend_from_slice(&discriminators::VERIFY_NON_MEMBERSHIP);
    non_membership_msg.serialize(&mut ix_data)?;

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

    // Get the return data from the light client
    // Light client should return timestamp for non-membership verification
    if let Some((program_id, return_data)) = get_return_data() {
        if program_id == client.client_program_id && return_data.len() >= ANCHOR_DISCRIMINATOR_SIZE
        {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&return_data[..ANCHOR_DISCRIMINATOR_SIZE]);
            return Ok(u64::from_le_bytes(bytes));
        }
    }

    // If no return data, the light client is not compliant with the interface
    // Real light clients MUST return timestamp for non-membership verification
    Err(ProgramError::InvalidAccountData.into())
}
