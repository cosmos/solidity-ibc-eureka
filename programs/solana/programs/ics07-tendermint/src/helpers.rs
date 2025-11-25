use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use borsh::BorshDeserialize;
use ibc_client_tendermint::types::{Header, Misbehaviour};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use ibc_proto::Protobuf;
use solana_ibc_types::borsh_header::HeaderWrapper;
use solana_program::log::sol_log_compute_units;

pub fn deserialize_header(bytes: &[u8]) -> Result<Header> {
    // Direct deserialization: bytes → Header in one pass (saves ~300k CU)
    msg!("deserialize_header: Starting direct deserialization");
    sol_log_compute_units();

    let wrapper =
        HeaderWrapper::try_from_slice(bytes).map_err(|_| error!(ErrorCode::InvalidHeader))?;

    msg!("deserialize_header: Direct deserialization complete");
    sol_log_compute_units();

    Ok(wrapper.0)
}

pub fn deserialize_merkle_proof(bytes: &[u8]) -> Result<MerkleProof> {
    <MerkleProof as Protobuf<RawMerkleProof>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidProof))
}

// TODO: switch to borsch
pub fn deserialize_misbehaviour(bytes: &[u8]) -> Result<Misbehaviour> {
    <Misbehaviour as Protobuf<RawMisbehaviour>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidHeader))
}

/// Closes an account and transfers its lamports to the recipient
pub fn close_account(account: &AccountInfo, recipient: &AccountInfo) -> Result<()> {
    // Zero out account data
    {
        let mut data = account.try_borrow_mut_data()?;
        data.fill(0);
    }

    // Transfer lamports in a separate scope to ensure borrows are dropped
    {
        let mut lamports = account.try_borrow_mut_lamports()?;
        let mut recipient_lamports = recipient.try_borrow_mut_lamports()?;

        **recipient_lamports = recipient_lamports
            .checked_add(**lamports)
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        **lamports = 0;
    }

    Ok(())
}
