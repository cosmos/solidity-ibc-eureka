//! IBC App Interface
//!
//! This module provides CPI helpers for invoking IBC app callbacks.
//!
//! # Example
//!
//! ```ignore
//! use solana_ibc_types::ibc_app::{on_recv_packet, OnRecvPacket};
//!
//! let cpi_ctx = CpiContext::new(
//!     ibc_app_program,
//!     OnRecvPacket {
//!         app_state,
//!         router_program,
//!         instructions_sysvar,
//!         payer,
//!         system_program,
//!     },
//! ).with_remaining_accounts(remaining_accounts);
//!
//! let ack = on_recv_packet(cpi_ctx, msg)?;
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_lang::solana_program::program::get_return_data;

use crate::utils::compute_discriminator;

// Re-export message types for convenient imports
pub use crate::app_msgs::{OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg};

const INSTRUCTION_DATA_CAPACITY: usize = 1024;

/// IBC app callback instruction names and discriminators
/// These MUST match the function names in your #[ibc_app] module
pub mod ibc_app_instructions {
    use crate::utils::compute_discriminator;

    /// Instruction name for receiving packets
    /// Your #[program] function MUST be named: `on_recv_packet`
    pub const ON_RECV_PACKET: &str = "on_recv_packet";

    /// Instruction name for acknowledgement callbacks
    /// Your #[program] function MUST be named: `on_acknowledgement_packet`
    pub const ON_ACKNOWLEDGEMENT_PACKET: &str = "on_acknowledgement_packet";

    /// Instruction name for timeout callbacks
    /// Your #[program] function MUST be named: `on_timeout_packet`
    pub const ON_TIMEOUT_PACKET: &str = "on_timeout_packet";

    pub fn on_recv_packet_discriminator() -> [u8; 8] {
        compute_discriminator(ON_RECV_PACKET)
    }

    pub fn on_acknowledgement_packet_discriminator() -> [u8; 8] {
        compute_discriminator(ON_ACKNOWLEDGEMENT_PACKET)
    }

    pub fn on_timeout_packet_discriminator() -> [u8; 8] {
        compute_discriminator(ON_TIMEOUT_PACKET)
    }
}

/// Accounts for `on_recv_packet` CPI call
#[derive(Clone)]
pub struct OnRecvPacket<'info> {
    pub app_state: AccountInfo<'info>,
    pub router_program: AccountInfo<'info>,
    pub instructions_sysvar: AccountInfo<'info>,
    pub payer: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
}

/// Accounts for `on_acknowledgement_packet` CPI call
#[derive(Clone)]
pub struct OnAcknowledgementPacket<'info> {
    pub app_state: AccountInfo<'info>,
    pub router_program: AccountInfo<'info>,
    pub instructions_sysvar: AccountInfo<'info>,
    pub payer: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
}

/// Accounts for `on_timeout_packet` CPI call
#[derive(Clone)]
pub struct OnTimeoutPacket<'info> {
    pub app_state: AccountInfo<'info>,
    pub router_program: AccountInfo<'info>,
    pub instructions_sysvar: AccountInfo<'info>,
    pub payer: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
}

impl<'info> anchor_lang::ToAccountMetas for OnRecvPacket<'info> {
    fn to_account_metas(&self, _is_signer: Option<bool>) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(*self.app_state.key, false),
            AccountMeta::new_readonly(*self.router_program.key, false),
            AccountMeta::new_readonly(*self.instructions_sysvar.key, false),
            AccountMeta::new(*self.payer.key, true),
            AccountMeta::new_readonly(*self.system_program.key, false),
        ]
    }
}

impl<'info> anchor_lang::ToAccountInfos<'info> for OnRecvPacket<'info> {
    fn to_account_infos(&self) -> Vec<AccountInfo<'info>> {
        vec![
            self.app_state.clone(),
            self.router_program.clone(),
            self.instructions_sysvar.clone(),
            self.payer.clone(),
            self.system_program.clone(),
        ]
    }
}

impl<'info> anchor_lang::ToAccountMetas for OnAcknowledgementPacket<'info> {
    fn to_account_metas(&self, _is_signer: Option<bool>) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(*self.app_state.key, false),
            AccountMeta::new_readonly(*self.router_program.key, false),
            AccountMeta::new_readonly(*self.instructions_sysvar.key, false),
            AccountMeta::new(*self.payer.key, true),
            AccountMeta::new_readonly(*self.system_program.key, false),
        ]
    }
}

impl<'info> anchor_lang::ToAccountInfos<'info> for OnAcknowledgementPacket<'info> {
    fn to_account_infos(&self) -> Vec<AccountInfo<'info>> {
        vec![
            self.app_state.clone(),
            self.router_program.clone(),
            self.instructions_sysvar.clone(),
            self.payer.clone(),
            self.system_program.clone(),
        ]
    }
}

impl<'info> anchor_lang::ToAccountMetas for OnTimeoutPacket<'info> {
    fn to_account_metas(&self, _is_signer: Option<bool>) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(*self.app_state.key, false),
            AccountMeta::new_readonly(*self.router_program.key, false),
            AccountMeta::new_readonly(*self.instructions_sysvar.key, false),
            AccountMeta::new(*self.payer.key, true),
            AccountMeta::new_readonly(*self.system_program.key, false),
        ]
    }
}

impl<'info> anchor_lang::ToAccountInfos<'info> for OnTimeoutPacket<'info> {
    fn to_account_infos(&self) -> Vec<AccountInfo<'info>> {
        vec![
            self.app_state.clone(),
            self.router_program.clone(),
            self.instructions_sysvar.clone(),
            self.payer.clone(),
            self.system_program.clone(),
        ]
    }
}

/// Invoke `on_recv_packet` on an IBC app via CPI.
///
/// Returns the acknowledgement bytes from the app.
///
/// # Example
///
/// ```ignore
/// let cpi_ctx = CpiContext::new(
///     ibc_app_program,
///     OnRecvPacket { app_state, router_program, instructions_sysvar, payer, system_program },
/// ).with_remaining_accounts(remaining);
///
/// let ack = on_recv_packet(cpi_ctx, msg)?;
/// ```
pub fn on_recv_packet<'info>(
    ctx: CpiContext<'_, '_, '_, 'info, OnRecvPacket<'info>>,
    msg: OnRecvPacketMsg,
) -> Result<Vec<u8>> {
    invoke_ibc_app(
        &ctx,
        compute_discriminator(ibc_app_instructions::ON_RECV_PACKET),
        msg,
    )?;

    // Get acknowledgement from return data
    match get_return_data() {
        Some((program_id, data)) if program_id == *ctx.program.key => Ok(data),
        _ => err!(IbcAppError::InvalidAppResponse),
    }
}

/// Invoke `on_acknowledgement_packet` on an IBC app via CPI.
///
/// # Example
///
/// ```ignore
/// let cpi_ctx = CpiContext::new(
///     ibc_app_program,
///     OnAcknowledgementPacket { app_state, router_program, instructions_sysvar, payer, system_program },
/// ).with_remaining_accounts(remaining);
///
/// on_acknowledgement_packet(cpi_ctx, msg)?;
/// ```
pub fn on_acknowledgement_packet<'info>(
    ctx: CpiContext<'_, '_, '_, 'info, OnAcknowledgementPacket<'info>>,
    msg: OnAcknowledgementPacketMsg,
) -> Result<()> {
    invoke_ibc_app(
        &ctx,
        compute_discriminator(ibc_app_instructions::ON_ACKNOWLEDGEMENT_PACKET),
        msg,
    )
}

/// Invoke `on_timeout_packet` on an IBC app via CPI.
///
/// # Example
///
/// ```ignore
/// let cpi_ctx = CpiContext::new(
///     ibc_app_program,
///     OnTimeoutPacket { app_state, router_program, instructions_sysvar, payer, system_program },
/// ).with_remaining_accounts(remaining);
///
/// on_timeout_packet(cpi_ctx, msg)?;
/// ```
pub fn on_timeout_packet<'info>(
    ctx: CpiContext<'_, '_, '_, 'info, OnTimeoutPacket<'info>>,
    msg: OnTimeoutPacketMsg,
) -> Result<()> {
    invoke_ibc_app(
        &ctx,
        compute_discriminator(ibc_app_instructions::ON_TIMEOUT_PACKET),
        msg,
    )
}

fn invoke_ibc_app<'info, T, M>(
    ctx: &CpiContext<'_, '_, '_, 'info, T>,
    discriminator: [u8; 8],
    msg: M,
) -> Result<()>
where
    T: anchor_lang::ToAccountMetas + anchor_lang::ToAccountInfos<'info>,
    M: AnchorSerialize,
{
    let mut data = Vec::with_capacity(INSTRUCTION_DATA_CAPACITY);
    data.extend_from_slice(&discriminator);
    msg.serialize(&mut data)?;

    let mut account_metas = ctx.accounts.to_account_metas(None);
    account_metas.extend(ctx.remaining_accounts.iter().map(|acc| AccountMeta {
        pubkey: *acc.key,
        is_signer: acc.is_signer,
        is_writable: acc.is_writable,
    }));

    let instruction = anchor_lang::solana_program::instruction::Instruction {
        program_id: *ctx.program.key,
        accounts: account_metas,
        data,
    };

    let mut account_infos = ctx.accounts.to_account_infos();
    account_infos.push(ctx.program.clone());
    account_infos.extend(ctx.remaining_accounts.iter().cloned());

    anchor_lang::solana_program::program::invoke(&instruction, &account_infos)?;

    Ok(())
}

#[error_code]
pub enum IbcAppError {
    #[msg("IBC app did not return valid response data")]
    InvalidAppResponse,
}
