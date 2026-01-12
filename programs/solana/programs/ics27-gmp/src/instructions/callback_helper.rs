//! Helper for forwarding IBC callbacks to upstream programs (e.g., IFT)

use crate::errors::GMPError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke;
use solana_ibc_proto::{GmpPacketData, Protobuf};

/// Forward an IBC callback to the sender program if remaining_accounts is provided.
///
/// # Arguments
/// * `remaining` - Remaining accounts where [0] is the callback program
/// * `payload_value` - Raw GMP packet payload bytes
/// * `discriminator_name` - Anchor discriminator name (e.g., "global:on_acknowledgement_packet")
/// * `msg` - The message to serialize and forward
///
/// # Returns
/// * `Ok(true)` - Callback was forwarded
/// * `Ok(false)` - No callback needed (remaining_accounts empty)
/// * `Err(_)` - Error during forwarding
pub fn forward_callback<M: AnchorSerialize>(
    remaining: &[AccountInfo],
    payload_value: &[u8],
    discriminator_name: &[u8],
    msg: &M,
) -> Result<bool> {
    if remaining.is_empty() {
        return Ok(false);
    }

    // Decode GMP packet to get sender (callback target)
    let gmp_packet =
        GmpPacketData::decode_vec(payload_value).map_err(|_| GMPError::InvalidPacketData)?;

    // Parse sender as Pubkey - this is the callback target
    let callback_program: Pubkey = gmp_packet
        .sender
        .as_ref()
        .parse()
        .map_err(|_| GMPError::InvalidSender)?;

    // Validate that remaining_accounts[0] is the callback program
    require!(
        remaining[0].key() == callback_program,
        GMPError::AccountKeyMismatch
    );

    // Build instruction data with Anchor discriminator
    let discriminator = solana_sha256_hasher::hash(discriminator_name);
    let mut ix_data = discriminator.to_bytes()[..8].to_vec();
    msg.serialize(&mut ix_data)?;

    // Build account metas from remaining_accounts, skipping the callback program at [0]
    let account_metas: Vec<AccountMeta> = remaining[1..]
        .iter()
        .map(|acc| AccountMeta {
            pubkey: *acc.key,
            is_signer: acc.is_signer,
            is_writable: acc.is_writable,
        })
        .collect();

    let instruction = Instruction {
        program_id: callback_program,
        accounts: account_metas,
        data: ix_data,
    };

    invoke(&instruction, remaining)?;

    Ok(true)
}
