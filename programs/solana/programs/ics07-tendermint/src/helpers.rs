use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::log::sol_log_compute_units;
use borsh::BorshDeserialize;
use ibc_client_tendermint::types::{Header, Misbehaviour};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use ibc_proto::Protobuf;
use solana_ibc_types::borsh_header::HeaderWrapper;

pub fn deserialize_header(bytes: &[u8]) -> Result<Header> {
    // Direct deserialization: bytes → Header in one pass (saves ~300k CU)
    msg!("deserialize_header: Starting direct deserialization");
    sol_log_compute_units();

    let wrapper = HeaderWrapper::try_from_slice(bytes)
        .map_err(|_| error!(ErrorCode::InvalidHeader))?;

    msg!("deserialize_header: Direct deserialization complete");
    sol_log_compute_units();

    Ok(wrapper.0)
}

pub fn deserialize_merkle_proof(bytes: &[u8]) -> Result<MerkleProof> {
    <MerkleProof as Protobuf<RawMerkleProof>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidProof))
}

pub fn deserialize_misbehaviour(bytes: &[u8]) -> Result<Misbehaviour> {
    <Misbehaviour as Protobuf<RawMisbehaviour>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidHeader))
}
