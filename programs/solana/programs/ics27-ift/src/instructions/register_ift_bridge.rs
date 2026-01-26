use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTBridgeRegistered;
use crate::state::{
    AccountVersion, CounterpartyChainType, IFTAppState, IFTBridge, RegisterIFTBridgeMsg,
};

#[derive(Accounts)]
#[instruction(msg: RegisterIFTBridgeMsg)]
pub struct RegisterIFTBridge<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge PDA (to be created)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTBridge::INIT_SPACE,
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), msg.client_id.as_bytes()],
        bump
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Authority with admin role
    pub authority: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn register_ift_bridge(
    ctx: Context<RegisterIFTBridge>,
    msg: RegisterIFTBridgeMsg,
) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    require!(!msg.client_id.is_empty(), IFTError::EmptyClientId);
    require!(
        msg.client_id.len() <= MAX_CLIENT_ID_LENGTH,
        IFTError::InvalidClientIdLength
    );
    require!(
        !msg.counterparty_ift_address.is_empty(),
        IFTError::EmptyCounterpartyAddress
    );
    require!(
        msg.counterparty_ift_address.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCounterpartyAddressLength
    );

    if msg.counterparty_chain_type == CounterpartyChainType::Cosmos {
        require!(
            !msg.counterparty_denom.is_empty(),
            IFTError::CosmosEmptyCounterpartyDenom
        );
        require!(
            !msg.cosmos_type_url.is_empty(),
            IFTError::CosmosEmptyTypeUrl
        );
        require!(
            !msg.cosmos_ica_address.is_empty(),
            IFTError::CosmosEmptyIcaAddress
        );
    }
    require!(
        msg.counterparty_denom.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCounterpartyDenomLength
    );
    require!(
        msg.cosmos_type_url.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCosmosTypeUrlLength
    );
    require!(
        msg.cosmos_ica_address.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCosmosIcaAddressLength
    );

    let bridge = &mut ctx.accounts.ift_bridge;
    bridge.version = AccountVersion::V1;
    bridge.bump = ctx.bumps.ift_bridge;
    bridge.mint = ctx.accounts.app_state.mint;
    bridge.client_id.clone_from(&msg.client_id);
    bridge
        .counterparty_ift_address
        .clone_from(&msg.counterparty_ift_address);
    bridge
        .counterparty_denom
        .clone_from(&msg.counterparty_denom);
    bridge.cosmos_type_url.clone_from(&msg.cosmos_type_url);
    bridge
        .cosmos_ica_address
        .clone_from(&msg.cosmos_ica_address);
    bridge.counterparty_chain_type = msg.counterparty_chain_type;
    bridge.active = true;

    let clock = Clock::get()?;
    emit!(IFTBridgeRegistered {
        mint: ctx.accounts.app_state.mint,
        client_id: msg.client_id,
        counterparty_ift_address: msg.counterparty_ift_address,
        counterparty_denom: msg.counterparty_denom,
        cosmos_type_url: msg.cosmos_type_url,
        cosmos_ica_address: msg.cosmos_ica_address,
        counterparty_chain_type: msg.counterparty_chain_type,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests;
