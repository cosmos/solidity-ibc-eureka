#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;
use anchor_lang::Result;

// FIXME:remove ed25519-consensus dep
//
use ibc_client_tendermint::types::{ConsensusState as IbcConsensusState, Header, Misbehaviour};
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use ibc_core_commitment_types::commitment::CommitmentRoot;
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_core_client_types::Height;
use ibc_primitives::prelude::*;
use tendermint_light_client_membership::KVPair;
use tendermint_light_client_misbehaviour::ClientState as TmClientState;
use tendermint_light_client_update_client::{ClientState as UpdateClientState, TrustThreshold};
use tendermint::{Time, Hash};
use tendermint::hash::Algorithm;
use time::OffsetDateTime;

declare_id!("8wQAC7oWLTxExhR49jYAzXZB39mu7WVVvkWJGgAMMjpV");

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClientState {
    pub chain_id: String,
    pub trust_level_numerator: u64,
    pub trust_level_denominator: u64,
    pub trusting_period: u64,
    pub unbonding_period: u64,
    pub max_clock_drift: u64,
    pub frozen_height: u64,
    pub latest_height: u64,
}

impl From<ClientState> for UpdateClientState {
    fn from(cs: ClientState) -> Self {
        UpdateClientState {
            chain_id: cs.chain_id,
            trust_level: TrustThreshold {
                numerator: cs.trust_level_numerator,
                denominator: cs.trust_level_denominator,
            },
            trusting_period_seconds: cs.trusting_period,
            unbonding_period_seconds: cs.unbonding_period,
            max_clock_drift_seconds: cs.max_clock_drift,
            frozen_height: if cs.frozen_height > 0 {
                Some(Height::new(0, cs.frozen_height).unwrap())
            } else {
                None
            },
            latest_height: Height::new(0, cs.latest_height).unwrap(),
        }
    }
}

impl From<ClientState> for TmClientState {
    fn from(cs: ClientState) -> Self {
        TmClientState {
            chain_id: cs.chain_id,
            trust_level: tendermint_light_client_misbehaviour::TrustThreshold {
                numerator: cs.trust_level_numerator,
                denominator: cs.trust_level_denominator,
            },
            trusting_period_seconds: cs.trusting_period,
            unbonding_period_seconds: cs.unbonding_period,
            max_clock_drift_seconds: cs.max_clock_drift,
            frozen_height: if cs.frozen_height > 0 {
                Some(Height::new(0, cs.frozen_height).unwrap())
            } else {
                None
            },
            latest_height: Height::new(0, cs.latest_height).unwrap(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ConsensusState {
    pub timestamp: u64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
}

impl From<ConsensusState> for IbcConsensusState {
    fn from(cs: ConsensusState) -> Self {
        let time = OffsetDateTime::from_unix_timestamp_nanos(
            cs.timestamp.try_into().expect("timestamp overflow"),
        )
        .expect("invalid timestamp");
        let seconds = time.unix_timestamp();
        let nanos = time.nanosecond();

        IbcConsensusState {
            timestamp: Time::from_unix_timestamp(seconds, nanos).expect("invalid time"),
            root: CommitmentRoot::from_bytes(&cs.root),
            next_validators_hash: Hash::from_bytes(
                Algorithm::Sha256,
                &cs.next_validators_hash,
            )
            .expect("invalid hash"),
        }
    }
}

impl From<IbcConsensusState> for ConsensusState {
    fn from(cs: IbcConsensusState) -> Self {
        ConsensusState {
            timestamp: cs.timestamp.unix_timestamp_nanos() as u64,
            root: cs.root.into_vec().try_into().expect("root must be 32 bytes"),
            next_validators_hash: cs.next_validators_hash.as_bytes().try_into()
                .expect("hash must be 32 bytes"),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateClientMsg {
    pub client_message: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MembershipMsg {
    pub height: u64,
    pub delay_time_period: u64,
    pub delay_block_period: u64,
    pub proof: Vec<u8>,
    pub path: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MisbehaviourMsg {
    pub client_id: String,
    pub misbehaviour: Vec<u8>, // Protobuf encoded Misbehaviour
}

fn deserialize_header(bytes: &[u8]) -> Result<Header> {
    <Header as Protobuf<RawHeader>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidHeader))
}

fn deserialize_merkle_proof(bytes: &[u8]) -> Result<MerkleProof> {
    <MerkleProof as Protobuf<RawMerkleProof>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidProof))
}

// Helper function to deserialize Misbehaviour from protobuf bytes
fn deserialize_misbehaviour(bytes: &[u8]) -> Result<Misbehaviour> {
    <Misbehaviour as Protobuf<RawMisbehaviour>>::decode_vec(bytes)
        .map_err(|_| error!(ErrorCode::InvalidHeader))
}

#[account]
pub struct ClientData {
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
    pub frozen: bool,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = payer, space = 8 + 1000)]
    pub client_data: Account<'info, ClientData>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateClient<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
}

#[derive(Accounts)]
pub struct VerifyMembership<'info> {
    pub client_data: Account<'info, ClientData>,
}

#[derive(Accounts)]
pub struct VerifyNonMembership<'info> {
    pub client_data: Account<'info, ClientData>,
}

#[derive(Accounts)]
pub struct SubmitMisbehaviour<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
}

#[program]
pub mod ics07_tendermint {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> Result<()> {
        let client_data = &mut ctx.accounts.client_data;
        client_data.client_state = client_state;
        client_data.consensus_state = consensus_state;
        client_data.frozen = false;
        Ok(())
    }

    pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<()> {
        let client_data = &mut ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientFrozen);

        // Deserialize the header from Protobuf
        let header = deserialize_header(&msg.client_message)?;

        let client_state: UpdateClientState = client_data.client_state.clone().into();
        let trusted_consensus_state: IbcConsensusState = client_data.consensus_state.clone().into();

        let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

        // Call the light client update function
        let output = tendermint_light_client_update_client::update_client(
            client_state,
            trusted_consensus_state,
            header,
            current_time,
        );

        client_data.client_state.latest_height = output.new_client_state.latest_height.revision_height();
        let new_consensus_state: ConsensusState = output.new_consensus_state.into();
        client_data.consensus_state = new_consensus_state;

        Ok(())
    }

    pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
        let client_data = &ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientFrozen);

        require!(
            msg.height <= client_data.client_state.latest_height,
            ErrorCode::InvalidHeight
        );

        let proof = deserialize_merkle_proof(&msg.proof)?;

        let kv_pair = KVPair::new(msg.path, msg.value);
        let app_hash = client_data.consensus_state.root;

        // Verify membership - this will panic internally if verification fails
        // If the function returns successfully, the proof is valid
        let _output = tendermint_light_client_membership::membership(
            app_hash,
            vec![(kv_pair, proof)].into_iter(),
        );

        Ok(())
    }

    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: MembershipMsg,
    ) -> Result<()> {
        let client_data = &ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientFrozen);

        require!(
            msg.height <= client_data.client_state.latest_height,
            ErrorCode::InvalidHeight
        );

        // Deserialize the proof from Protobuf
        let proof = deserialize_merkle_proof(&msg.proof)?;

        // For non-membership, the value should be empty
        let kv_pair = KVPair::new(msg.path, vec![]);
        let app_hash = client_data.consensus_state.root;

        // Verify non-membership (absence of the key)
        // Verify membership - this will panic internally if verification fails
        // If the function returns successfully, the proof is valid
        let _output = tendermint_light_client_membership::membership(
            app_hash,
            vec![(kv_pair, proof)].into_iter(),
        );

        Ok(())
    }

    pub fn submit_misbehaviour(
        ctx: Context<SubmitMisbehaviour>,
        msg: MisbehaviourMsg,
    ) -> Result<()> {
        let client_data = &mut ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientAlreadyFrozen);

        let misbehaviour = deserialize_misbehaviour(&msg.misbehaviour)?;
        let client_state: TmClientState = client_data.client_state.clone().into();

        // NOTE: Naive implementation as we'use the current consensus state as the trusted state
        // In a production implementation, you would need to store historical consensus states
        // and retrieve the ones corresponding to the trusted heights
        let trusted_consensus_state_1: IbcConsensusState = client_data.consensus_state.clone().into();
        let trusted_consensus_state_2: IbcConsensusState = client_data.consensus_state.clone().into();

        // Get current time for verification
        let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

        // Call the misbehaviour verification function
        let output = tendermint_light_client_misbehaviour::check_for_misbehaviour(
            client_state,
            &misbehaviour,
            trusted_consensus_state_1,
            trusted_consensus_state_2,
            current_time,
        );

        // If we reach here, misbehaviour was detected
        // Freeze the client at the current height
        client_data.frozen = true;
        client_data.client_state.frozen_height = client_data.client_state.latest_height;

        msg!("Misbehaviour detected at heights {:?} and {:?}",
            output.trusted_height_1,
            output.trusted_height_2
        );

        Ok(())
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Client is frozen")]
    ClientFrozen,
    #[msg("Client is already frozen")]
    ClientAlreadyFrozen,
    #[msg("Invalid header")]
    InvalidHeader,
    #[msg("Invalid height")]
    InvalidHeight,
    #[msg("Invalid proof")]
    InvalidProof,
    #[msg("Invalid client ID")]
    InvalidClientId,
    #[msg("Verification failed")]
    VerificationFailed,
}
