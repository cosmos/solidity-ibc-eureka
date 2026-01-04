use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount};
use ics27_gmp::constants::GMP_PORT_ID;

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTTransferInitiated;
use crate::evm_selectors::{IFT_MINT_DISCRIMINATOR, IFT_MINT_SELECTOR};
use crate::gmp_cpi::{SendGmpCallAccounts, SendGmpCallMsg};
use crate::state::{CounterpartyChainType, IFTAppState, IFTBridge, IFTTransferMsg};

#[derive(Accounts)]
#[instruction(msg: IFTTransferMsg)]
pub struct IFTTransfer<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge for the destination
    #[account(
        mut,
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), msg.client_id.as_bytes()],
        bump = ift_bridge.bump,
        constraint = ift_bridge.active @ IFTError::BridgeNotActive
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// SPL Token mint
    #[account(
        mut,
        address = app_state.mint
    )]
    pub mint: Account<'info, Mint>,

    /// Sender's token account
    #[account(
        mut,
        constraint = sender_token_account.mint == mint.key() @ IFTError::TokenAccountOwnerMismatch,
        constraint = sender_token_account.owner == sender.key() @ IFTError::TokenAccountOwnerMismatch
    )]
    pub sender_token_account: Account<'info, TokenAccount>,

    /// Sender who owns the tokens
    pub sender: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,

    /// GMP program
    /// CHECK: Validated against stored `gmp_program` in `app_state`
    #[account(
        address = app_state.gmp_program @ IFTError::InvalidGmpProgram
    )]
    pub gmp_program: AccountInfo<'info>,

    /// GMP app state PDA
    /// CHECK: Validated by GMP program via CPI
    #[account(
        mut,
        seeds = [solana_ibc_types::GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump,
        seeds::program = gmp_program.key()
    )]
    pub gmp_app_state: AccountInfo<'info>,

    /// Router program
    pub router_program: Program<'info, ics26_router::program::Ics26Router>,

    /// Router state account
    /// CHECK: Router program validates this
    #[account()]
    pub router_state: AccountInfo<'info>,

    /// Client sequence account for packet sequencing
    /// CHECK: Router program validates this
    #[account(mut)]
    pub client_sequence: AccountInfo<'info>,

    /// Packet commitment account to be created
    /// CHECK: Router program validates this
    #[account(mut)]
    pub packet_commitment: AccountInfo<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    /// GMP's IBC app registration account
    /// CHECK: Router program validates this
    #[account()]
    pub gmp_ibc_app: AccountInfo<'info>,

    /// IBC client account
    /// CHECK: Router program validates this
    #[account()]
    pub ibc_client: AccountInfo<'info>,
}

pub fn ift_transfer(ctx: Context<IFTTransfer>, msg: IFTTransferMsg) -> Result<u64> {
    let clock = Clock::get()?;

    // Validate inputs
    require!(msg.amount > 0, IFTError::ZeroAmount);
    require!(!msg.receiver.is_empty(), IFTError::EmptyReceiver);
    require!(
        msg.receiver.len() <= MAX_RECEIVER_LENGTH,
        IFTError::InvalidReceiver
    );

    // Calculate timeout (default 15 minutes)
    let timeout = if msg.timeout_timestamp == 0 {
        clock.unix_timestamp + DEFAULT_TIMEOUT_DURATION
    } else {
        require!(
            msg.timeout_timestamp > clock.unix_timestamp,
            IFTError::TimeoutInPast
        );
        require!(
            msg.timeout_timestamp <= clock.unix_timestamp + MAX_TIMEOUT_DURATION,
            IFTError::TimeoutTooLong
        );
        msg.timeout_timestamp
    };

    // Burn tokens from sender
    let burn_accounts = Burn {
        mint: ctx.accounts.mint.to_account_info(),
        from: ctx.accounts.sender_token_account.to_account_info(),
        authority: ctx.accounts.sender.to_account_info(),
    };
    let burn_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), burn_accounts);
    token::burn(burn_ctx, msg.amount)?;

    // Construct mint call payload based on counterparty chain type
    let mint_call_payload = construct_mint_call(
        ctx.accounts.ift_bridge.counterparty_chain_type,
        &ctx.accounts.ift_bridge.counterparty_ift_address,
        &msg.receiver,
        msg.amount,
    )?;

    // Send GMP call via CPI
    let gmp_accounts = SendGmpCallAccounts {
        gmp_program: ctx.accounts.gmp_program.clone(),
        gmp_app_state: ctx.accounts.gmp_app_state.clone(),
        sender: ctx.accounts.sender.to_account_info(),
        payer: ctx.accounts.payer.to_account_info(),
        router_program: ctx.accounts.router_program.to_account_info(),
        router_state: ctx.accounts.router_state.clone(),
        client_sequence: ctx.accounts.client_sequence.clone(),
        packet_commitment: ctx.accounts.packet_commitment.clone(),
        instruction_sysvar: ctx.accounts.instruction_sysvar.clone(),
        ibc_app: ctx.accounts.gmp_ibc_app.clone(),
        client: ctx.accounts.ibc_client.clone(),
        system_program: ctx.accounts.system_program.to_account_info(),
    };

    let gmp_msg = SendGmpCallMsg {
        source_client: msg.client_id.clone(),
        timeout_timestamp: timeout,
        receiver: ctx.accounts.ift_bridge.counterparty_ift_address.clone(),
        payload: mint_call_payload,
    };

    let sequence = crate::gmp_cpi::send_gmp_call(gmp_accounts, gmp_msg)?;

    emit!(IFTTransferInitiated {
        mint: ctx.accounts.app_state.mint,
        client_id: msg.client_id.clone(),
        sequence,
        sender: ctx.accounts.sender.key(),
        receiver: msg.receiver,
        amount: msg.amount,
        timeout_timestamp: timeout,
    });

    Ok(sequence)
}

/// Construct the mint call payload based on counterparty chain type
fn construct_mint_call(
    chain_type: CounterpartyChainType,
    counterparty_address: &str,
    receiver: &str,
    amount: u64,
) -> Result<Vec<u8>> {
    match chain_type {
        CounterpartyChainType::Evm => construct_evm_mint_call(receiver, amount),
        CounterpartyChainType::Cosmos => Ok(construct_cosmos_mint_call(
            counterparty_address,
            receiver,
            amount,
        )),
        CounterpartyChainType::Solana => construct_solana_mint_call(receiver, amount),
    }
}

/// Construct ABI-encoded call to iftMint(address, uint256) for EVM chains
fn construct_evm_mint_call(receiver: &str, amount: u64) -> Result<Vec<u8>> {
    let mut payload = Vec::with_capacity(68);

    // Function selector: keccak256("iftMint(address,uint256)")[:4]
    // Generated at compile time by build.rs
    payload.extend_from_slice(&IFT_MINT_SELECTOR);

    // Parse receiver as hex address (remove 0x prefix if present)
    let receiver_hex = receiver.trim_start_matches("0x");
    let receiver_bytes = hex_to_bytes(receiver_hex).map_err(|()| IFTError::InvalidReceiver)?;

    // Pad receiver address to 32 bytes (left-padded with zeros)
    let mut padded_receiver = [0u8; 32];
    let start = 32 - receiver_bytes.len().min(20);
    padded_receiver[start..start + receiver_bytes.len().min(20)]
        .copy_from_slice(&receiver_bytes[..receiver_bytes.len().min(20)]);
    payload.extend_from_slice(&padded_receiver);

    // Amount as u256 (32 bytes, big-endian, left-padded)
    let mut amount_bytes = [0u8; 32];
    amount_bytes[24..32].copy_from_slice(&amount.to_be_bytes());
    payload.extend_from_slice(&amount_bytes);

    Ok(payload)
}

/// Construct protojson-encoded `MsgIFTMint` for Cosmos chains
fn construct_cosmos_mint_call(denom: &str, receiver: &str, amount: u64) -> Vec<u8> {
    // Build protojson for Cosmos SDK's MsgIFTMint
    let msg = format!(
        r#"{{"@type":"/cosmos.ift.v1.MsgIFTMint","denom":"{denom}","receiver":"{receiver}","amount":"{amount}"}}"#
    );
    msg.into_bytes()
}

/// Construct Solana instruction data for IFT mint
fn construct_solana_mint_call(receiver: &str, amount: u64) -> Result<Vec<u8>> {
    use std::str::FromStr;

    let receiver_pubkey = Pubkey::from_str(receiver).map_err(|_| IFTError::InvalidReceiver)?;

    let mut payload = Vec::with_capacity(48); // 8 discriminator + 32 pubkey + 8 amount
    payload.extend_from_slice(&IFT_MINT_DISCRIMINATOR);
    payload.extend_from_slice(&receiver_pubkey.to_bytes());
    payload.extend_from_slice(&amount.to_le_bytes());
    Ok(payload)
}

/// Simple hex string to bytes conversion
#[allow(clippy::manual_is_multiple_of)]
fn hex_to_bytes(hex: &str) -> std::result::Result<Vec<u8>, ()> {
    if hex.len() % 2 != 0 {
        return Err(());
    }

    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

#[cfg(test)]
mod tests;
