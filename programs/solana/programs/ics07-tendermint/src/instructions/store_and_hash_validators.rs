use crate::conversions::borsh_to_validator_set;
use crate::error::ErrorCode;
use crate::StoreAndHashValidators;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hash;
use solana_ibc_types::borsh_header::BorshValidatorSet;

use tendermint::merkle;
use tendermint_light_client_solana::SolanaSha256;

/// Parameters for storing and hashing validators
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct StoreValidatorsParams {
    /// Simple SHA256 hash of `validators_bytes` (for PDA derivation)
    pub simple_hash: [u8; 32],
    /// Borsh-serialized validators bytes
    pub validators_bytes: Vec<u8>,
}

pub fn store_and_hash_validators(
    ctx: Context<StoreAndHashValidators>,
    params: StoreValidatorsParams,
) -> Result<()> {
    let storage = &mut ctx.accounts.validators_storage;

    // Verify that the provided simple_hash matches the actual hash of validators_bytes
    let computed_hash = hash(&params.validators_bytes).to_bytes();
    require!(
        params.simple_hash == computed_hash,
        ErrorCode::InvalidSimpleHash
    );

    let borsh_validator_set: BorshValidatorSet =
        borsh::BorshDeserialize::try_from_slice(&params.validators_bytes)
            .map_err(|_| ErrorCode::ValidatorsDeserializationFailed)?;

    let validator_set = borsh_to_validator_set(borsh_validator_set)
        .map_err(|_| ErrorCode::ValidatorsDeserializationFailed)?;

    let validator_bytes: Vec<Vec<u8>> = validator_set
        .validators()
        .iter()
        .map(tendermint::validator::Info::hash_bytes)
        .collect();

    let merkle_hash = merkle::simple_hash_from_byte_vectors::<SolanaSha256>(&validator_bytes);

    storage.simple_hash = params.simple_hash;
    storage.merkle_hash = merkle_hash;
    storage.validators_bytes = params.validators_bytes;

    Ok(())
}

#[cfg(test)]
mod tests;
