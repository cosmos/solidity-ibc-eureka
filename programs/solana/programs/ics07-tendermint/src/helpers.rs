use crate::error::ErrorCode;
use crate::state::ConsensusStateStore;
use crate::types::{ClientState, MembershipMsg};
use anchor_lang::prelude::*;
use ibc_client_tendermint::types::{Header, Misbehaviour};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};

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
    consensus_state_store: &ConsensusStateStore,
    msg: &MembershipMsg,
) -> Result<()> {
    require!(!client_state.is_frozen(), ErrorCode::ClientFrozen);

    require!(
        consensus_state_store.height == msg.height,
        ErrorCode::ProofHeightNotFound
    );

    require!(
        msg.height <= client_state.latest_height.revision_height,
        ErrorCode::InvalidHeight
    );

    if msg.delay_time_period > 0 || msg.delay_block_period > 0 {
        let current_timestamp = Clock::get()?.unix_timestamp as u64;
        let current_height = client_state.latest_height.revision_height;

        let proof_timestamp = consensus_state_store.consensus_state.timestamp / 1_000_000_000; // Convert nanos to seconds
        let time_elapsed = current_timestamp.saturating_sub(proof_timestamp);
        let blocks_elapsed = current_height.saturating_sub(msg.height);

        require!(
            time_elapsed >= msg.delay_time_period,
            ErrorCode::InsufficientTimeDelay
        );
        require!(
            blocks_elapsed >= msg.delay_block_period,
            ErrorCode::InsufficientBlockDelay
        );
    }

    Ok(())
}
