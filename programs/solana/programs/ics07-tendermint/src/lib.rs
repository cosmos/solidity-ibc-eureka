#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;
use anchor_lang::Result;

// FIXME:remove ed25519-consensus dep
//
use anchor_lang::solana_program::keccak::hash as keccak256;
use ibc_client_tendermint::types::{ConsensusState as IbcConsensusState, Header, Misbehaviour};
use ibc_core_client_types::Height;
use ibc_core_commitment_types::commitment::CommitmentRoot;
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_primitives::prelude::*;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use ibc_proto::{ibc::lightclients::tendermint::v1::Header as RawHeader, Protobuf};
use tendermint::Time;
use tendermint_light_client_membership::KVPair;
use tendermint_light_client_misbehaviour;
use tendermint_light_client_misbehaviour::ClientState as TmClientState;
use tendermint_light_client_update_client::{ClientState as UpdateClientState, TrustThreshold};
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
            is_frozen: cs.frozen_height > 0,
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
            next_validators_hash: tendermint::Hash::Sha256(
                cs.next_validators_hash.try_into().expect("invalid hash"),
            ),
        }
    }
}

impl From<IbcConsensusState> for ConsensusState {
    fn from(cs: IbcConsensusState) -> Self {
        ConsensusState {
            timestamp: cs.timestamp.unix_timestamp_nanos() as u64,
            root: cs
                .root
                .into_vec()
                .try_into()
                .expect("root must be 32 bytes"),
            next_validators_hash: cs
                .next_validators_hash
                .as_bytes()
                .try_into()
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
    <Header as Protobuf<RawHeader>>::decode_vec(bytes).map_err(|_| error!(ErrorCode::InvalidHeader))
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

// Helper function to validate proof parameters
fn validate_proof_params(
    client_data: &ClientData,
    consensus_state_store: &ConsensusStateStore,
    msg: &MembershipMsg,
) -> Result<()> {
    require!(!client_data.frozen, ErrorCode::ClientFrozen);

    // Verify that the consensus state is for the requested height
    require!(
        consensus_state_store.height == msg.height,
        ErrorCode::InvalidHeight
    );

    require!(
        msg.height <= client_data.client_state.latest_height,
        ErrorCode::InvalidHeight
    );

    // Check delay period if specified
    if msg.delay_time_period > 0 || msg.delay_block_period > 0 {
        let current_timestamp = Clock::get()?.unix_timestamp as u64;
        let current_height = client_data.client_state.latest_height;

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

#[account]
pub struct ClientData {
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
    pub frozen: bool,
}

#[account]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = payer, space = 8 + 1000)]
    pub client_data: Account<'info, ClientData>,
    #[account(
        init,
        payer = payer,
        space = 8 + 8 + 8 + 32 + 32, // discriminator + height + timestamp + root + next_validators_hash
        seeds = [b"consensus_state", client_data.key().as_ref(), &0u64.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(msg: UpdateClientMsg)]
pub struct UpdateClient<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + 8 + 8 + 32 + 32,
        seeds = [b"consensus_state", client_data.key().as_ref(), &client_data.client_state.latest_height.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct VerifyMembership<'info> {
    pub client_data: Account<'info, ClientData>,
    /// Consensus state at the proof height
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
pub struct VerifyNonMembership<'info> {
    pub client_data: Account<'info, ClientData>,
    /// Consensus state at the proof height
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
#[instruction(msg: MisbehaviourMsg)]
pub struct SubmitMisbehaviour<'info> {
    #[account(mut)]
    pub client_data: Account<'info, ClientData>,
    /// Consensus state at the trusted height of header 1
    pub trusted_consensus_state_1: Account<'info, ConsensusStateStore>,
    /// Consensus state at the trusted height of header 2
    pub trusted_consensus_state_2: Account<'info, ConsensusStateStore>,
}

#[derive(Accounts)]
pub struct GetConsensusStateHash<'info> {
    pub client_data: Account<'info, ClientData>,
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
}

#[program]
pub mod ics07_tendermint {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> Result<()> {
        require!(!client_state.chain_id.is_empty(), ErrorCode::InvalidChainId);

        require!(
            client_state.trust_level_numerator > 0
                && client_state.trust_level_numerator <= client_state.trust_level_denominator
                && client_state.trust_level_denominator > 0,
            ErrorCode::InvalidTrustLevel
        );

        require!(
            client_state.trusting_period > 0
                && client_state.unbonding_period > 0
                && client_state.trusting_period < client_state.unbonding_period,
            ErrorCode::InvalidPeriods
        );

        require!(
            client_state.max_clock_drift > 0,
            ErrorCode::InvalidMaxClockDrift
        );

        require!(client_state.latest_height > 0, ErrorCode::InvalidHeight);

        let client_data = &mut ctx.accounts.client_data;
        client_data.client_state = client_state.clone();
        client_data.consensus_state = consensus_state.clone();
        client_data.frozen = false;

        let consensus_state_store = &mut ctx.accounts.consensus_state_store;
        consensus_state_store.height = client_state.latest_height;
        consensus_state_store.consensus_state = consensus_state;

        Ok(())
    }

    pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<()> {
        let client_data = &mut ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientFrozen);

        let header = deserialize_header(&msg.client_message)?;

        let client_state: UpdateClientState = client_data.client_state.clone().into();
        let trusted_consensus_state: IbcConsensusState = client_data.consensus_state.clone().into();

        let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

        let output = tendermint_light_client_update_client::update_client(
            &client_state,
            &trusted_consensus_state,
            header,
            current_time,
        )
        .map_err(|e| {
            msg!("Update client failed: {:?}", e);
            error!(ErrorCode::UpdateClientFailed)
        })?;

        client_data.client_state.latest_height = output.latest_height.revision_height();
        let new_consensus_state: ConsensusState = output.new_consensus_state.clone().into();
        client_data.consensus_state = new_consensus_state.clone();

        let consensus_state_store = &mut ctx.accounts.consensus_state_store;
        consensus_state_store.height = output.latest_height.revision_height();
        consensus_state_store.consensus_state = new_consensus_state;

        Ok(())
    }

    pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
        let client_data = &ctx.accounts.client_data;
        let consensus_state_store = &ctx.accounts.consensus_state_at_height;

        validate_proof_params(client_data, consensus_state_store, &msg)?;

        let proof = deserialize_merkle_proof(&msg.proof)?;
        let kv_pair = KVPair::new(vec![msg.path.clone()], msg.value);
        let app_hash = consensus_state_store.consensus_state.root;

        tendermint_light_client_membership::membership(
            app_hash,
            &[(kv_pair, proof)],
        )
        .map_err(|e| {
            msg!("Membership verification failed: {:?}", e);
            error!(ErrorCode::MembershipVerificationFailed)
        })?;

        Ok(())
    }

    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: MembershipMsg,
    ) -> Result<()> {
        let client_data = &ctx.accounts.client_data;
        let consensus_state_store = &ctx.accounts.consensus_state_at_height;

        validate_proof_params(client_data, consensus_state_store, &msg)?;

        // For non-membership, the value must be empty
        require!(msg.value.is_empty(), ErrorCode::InvalidValue);

        let proof = deserialize_merkle_proof(&msg.proof)?;
        let kv_pair = KVPair::new(vec![msg.path.clone()], vec![]);
        let app_hash = consensus_state_store.consensus_state.root;

        tendermint_light_client_membership::membership(
            app_hash,
            &[(kv_pair, proof)],
        )
        .map_err(|e| {
            msg!("Non-membership verification failed: {:?}", e);
            error!(ErrorCode::NonMembershipVerificationFailed)
        })?;

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

        let trusted_consensus_state_1: IbcConsensusState = ctx
            .accounts
            .trusted_consensus_state_1
            .consensus_state
            .clone()
            .into();
        let trusted_consensus_state_2: IbcConsensusState = ctx
            .accounts
            .trusted_consensus_state_2
            .consensus_state
            .clone()
            .into();

        let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

        let output = tendermint_light_client_misbehaviour::check_for_misbehaviour(
            &client_state,
            &misbehaviour,
            trusted_consensus_state_1,
            trusted_consensus_state_2,
            current_time,
        )
        .map_err(|e| {
            msg!("Misbehaviour check failed: {:?}", e);
            error!(ErrorCode::MisbehaviourFailed)
        })?;

        require!(
            ctx.accounts.trusted_consensus_state_1.height
                == output.trusted_height_1.revision_height(),
            ErrorCode::InvalidHeight
        );
        require!(
            ctx.accounts.trusted_consensus_state_2.height
                == output.trusted_height_2.revision_height(),
            ErrorCode::InvalidHeight
        );

        // If we reach here, misbehaviour was detected
        // Freeze the client at the current height
        client_data.frozen = true;
        client_data.client_state.frozen_height = client_data.client_state.latest_height;

        msg!(
            "Misbehaviour detected at heights {:?} and {:?}",
            output.trusted_height_1,
            output.trusted_height_2
        );

        Ok(())
    }

    pub fn get_consensus_state_hash(
        ctx: Context<GetConsensusStateHash>,
        revision_height: u64,
    ) -> Result<[u8; 32]> {
        let consensus_state_store = &ctx.accounts.consensus_state_store;

        require!(
            consensus_state_store.height == revision_height,
            ErrorCode::InvalidHeight
        );

        // Optimized for Solana: Direct concatenation without unnecessary padding
        // Total: 8 + 32 + 32 = 72 bytes
        let mut data = Vec::with_capacity(72);

        // Timestamp (8 bytes)
        data.extend_from_slice(
            &consensus_state_store
                .consensus_state
                .timestamp
                .to_le_bytes(),
        );

        // Root (32 bytes)
        data.extend_from_slice(&consensus_state_store.consensus_state.root);

        // Next validators hash (32 bytes)
        data.extend_from_slice(&consensus_state_store.consensus_state.next_validators_hash);

        // Native Solana syscall
        let hash_result = keccak256(&data);
        let hash_bytes: [u8; 32] = hash_result.to_bytes();

        Ok(hash_bytes)
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
    #[msg("Update client failed")]
    UpdateClientFailed,
    #[msg("Misbehaviour check failed")]
    MisbehaviourFailed,
    #[msg("Verification failed")]
    VerificationFailed,
    #[msg("Membership verification failed")]
    MembershipVerificationFailed,
    #[msg("Non-membership verification failed")]
    NonMembershipVerificationFailed,
    #[msg("Insufficient time delay")]
    InsufficientTimeDelay,
    #[msg("Insufficient block delay")]
    InsufficientBlockDelay,
    #[msg("Invalid value for non-membership proof")]
    InvalidValue,
    #[msg("Invalid chain ID")]
    InvalidChainId,
    #[msg("Invalid trust level")]
    InvalidTrustLevel,
    #[msg("Invalid periods: trusting period must be positive and less than unbonding period")]
    InvalidPeriods,
    #[msg("Invalid max clock drift")]
    InvalidMaxClockDrift,
    #[msg("Serialization error")]
    SerializationError,
}
