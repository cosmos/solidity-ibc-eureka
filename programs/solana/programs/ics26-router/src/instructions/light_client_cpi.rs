use anchor_lang::prelude::*;
use crate::state::{ClientRegistry, ClientType};
use crate::errors::RouterError;

/// Message structure for light client membership verification
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MembershipMsg {
    pub height: u64,
    pub delay_time_period: u64,
    pub delay_block_period: u64,
    pub proof: Vec<u8>,
    pub path: Vec<u8>,
    pub value: Vec<u8>,
}

/// Message structure for light client non-membership verification
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct NonMembershipMsg {
    pub height: u64,
    pub delay_time_period: u64,
    pub delay_block_period: u64,
    pub proof: Vec<u8>,
    pub path: Vec<u8>,
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
        // Future client types can be added here
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
        // Future client types can be added here
    }
}

fn verify_tendermint_membership(
    _light_client_accounts: &LightClientVerification,
    membership_msg: MembershipMsg,
) -> Result<u64> {
    // TODO: Implement actual CPI call to ICS07 Tendermint program

    msg!("TODO: Verify membership proof via CPI to ICS07 Tendermint");
    msg!("Proof height: {}", membership_msg.height);
    msg!("Proof path length: {}", membership_msg.path.len());
    msg!("Proof value length: {}", membership_msg.value.len());

    // Return timestamp for now (placeholder)
    Ok(membership_msg.height)
}

/// Verify non-membership using ICS07 Tendermint light client
fn verify_tendermint_non_membership(
    _light_client_accounts: &LightClientVerification,
    non_membership_msg: NonMembershipMsg,
) -> Result<u64> {
    // TODO: Implement actual CPI call to ICS07 Tendermint program
    // This would look something like:

    msg!("TODO: Verify non-membership proof via CPI to ICS07 Tendermint");
    msg!("Proof height: {}", non_membership_msg.height);
    msg!("Proof path length: {}", non_membership_msg.path.len());

    Ok(non_membership_msg.height)
}

/// Helper function to construct IBC commitment path
pub fn construct_commitment_path(
    _client_id: &str,
    sequence: u64,
    port_id: &str,
    dest_port: &str,
) -> Vec<u8> {
    // ICS24 path: commitments/ports/{port_id}/channels/{channel_id}/sequences/{sequence}
    // For now, simplified path construction
    format!("commitments/ports/{}/channels/{}/sequences/{}", port_id, dest_port, sequence)
        .into_bytes()
}

pub fn construct_receipt_path(
    _client_id: &str,
    sequence: u64,
    port_id: &str,
    dest_port: &str,
) -> Vec<u8> {
    // ICS24 path: receipts/ports/{port_id}/channels/{channel_id}/sequences/{sequence}
    // For now, simplified path construction
    format!("receipts/ports/{}/channels/{}/sequences/{}", port_id, dest_port, sequence)
        .into_bytes()
}

pub fn construct_ack_path(
    _client_id: &str,
    sequence: u64,
    port_id: &str,
    dest_port: &str,
) -> Vec<u8> {
    // ICS24 path: acks/ports/{port_id}/channels/{channel_id}/sequences/{sequence}
    // For now, simplified path construction
    format!("acks/ports/{}/channels/{}/sequences/{}", port_id, dest_port, sequence)
        .into_bytes()
}
