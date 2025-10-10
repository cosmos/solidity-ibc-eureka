use crate::error::ErrorCode;
use crate::types::ClientState;
use anchor_lang::prelude::*;
use ibc_client_tendermint::types::{Header, Misbehaviour};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use ics25_handler::MembershipMsg;

pub fn deserialize_header(bytes: &[u8]) -> Result<Header> {
    <Header as Protobuf<RawHeader>>::decode_vec(bytes).map_err(|_| error!(ErrorCode::InvalidHeader))
}

pub fn deserialize_merkle_proof(bytes: &[u8]) -> Result<MerkleProof> {
    <MerkleProof as Protobuf<RawMerkleProof>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidProof))
}

pub fn deserialize_misbehaviour(bytes: &[u8]) -> Result<Misbehaviour> {
    <Misbehaviour as Protobuf<RawMisbehaviour>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidHeader))
}

pub fn validate_proof_params(
    client_state: &Account<ClientState>,
    msg: &MembershipMsg,
) -> Result<()> {
    require!(!client_state.is_frozen(), ErrorCode::ClientFrozen);

    require!(
        msg.height <= client_state.latest_height.revision_height,
        ErrorCode::InvalidHeight
    );

    require!(
        msg.delay_time_period == 0 && msg.delay_block_period == 0,
        ErrorCode::NonZeroDelay
    );

    Ok(())
}
