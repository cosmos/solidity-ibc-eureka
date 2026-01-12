use crate::errors::GMPError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke;
use solana_ibc_proto::{GmpPacketData, Protobuf};

/// Forward IBC callback to sender program. Returns false if no remaining_accounts.
pub fn forward_callback<M: AnchorSerialize>(
    remaining: &[AccountInfo],
    payload_value: &[u8],
    discriminator_name: &[u8],
    msg: &M,
) -> Result<bool> {
    if remaining.is_empty() {
        return Ok(false);
    }

    let gmp_packet =
        GmpPacketData::decode_vec(payload_value).map_err(|_| GMPError::InvalidPacketData)?;

    let callback_program: Pubkey = gmp_packet
        .sender
        .as_ref()
        .parse()
        .map_err(|_| GMPError::InvalidSender)?;

    require!(
        remaining[0].key() == callback_program,
        GMPError::AccountKeyMismatch
    );

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
