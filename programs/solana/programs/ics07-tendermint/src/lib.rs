#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;

// FIXME:remove ed25519-consensus dep
//
use ibc_client_tendermint::types::{ConsensusState as TmConsensusState, Header};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_core_client_types::Height;
use ibc_primitives::prelude::*;
use tendermint_light_client_membership::{KVPair, MembershipOutput};
use tendermint_light_client_misbehaviour::{ClientState as TmClientState, MisbehaviourOutput};
use tendermint_light_client_update_client::{ClientState as UpdateClientState, ConsensusState as UpdateConsensusState, UpdateClientOutput};

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
            trust_level_numerator: cs.trust_level_numerator,
            trust_level_denominator: cs.trust_level_denominator,
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
            trust_level_numerator: cs.trust_level_numerator,
            trust_level_denominator: cs.trust_level_denominator,
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

impl From<ConsensusState> for UpdateConsensusState {
    fn from(cs: ConsensusState) -> Self {
        UpdateConsensusState {
            timestamp_nanos: cs.timestamp,
            app_hash: cs.root,
            next_validators_hash: cs.next_validators_hash,
        }
    }
}

impl From<UpdateConsensusState> for ConsensusState {
    fn from(cs: UpdateConsensusState) -> Self {
        ConsensusState {
            timestamp: cs.timestamp_nanos,
            root: cs.app_hash,
            next_validators_hash: cs.next_validators_hash,
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
    pub header_1: Vec<u8>,
    pub header_2: Vec<u8>,
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

        let header: Header = borsh::BorshDeserialize::try_from_slice(&msg.client_message)
            .map_err(|_| error!(ErrorCode::InvalidHeader))?;

        let client_state: UpdateClientState = client_data.client_state.clone().into();
        let trusted_consensus_state: UpdateConsensusState = client_data.consensus_state.clone().into();

        let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

        let output = tendermint_light_client_update_client::verify_header_update(
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

        let proof: MerkleProof = borsh::BorshDeserialize::try_from_slice(&msg.proof)
            .map_err(|_| error!(ErrorCode::InvalidProof))?;

        let kv_pair = KVPair::new(msg.path, msg.value);
        let app_hash = client_data.consensus_state.root;

        let _output = tendermint_light_client_membership::verify_membership(
            app_hash,
            vec![(kv_pair, proof)],
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

        let proof: MerkleProof = borsh::BorshDeserialize::try_from_slice(&msg.proof)
            .map_err(|_| error!(ErrorCode::InvalidProof))?;

        let kv_pair = KVPair::non_membership(msg.path);
        let app_hash = client_data.consensus_state.root;

        let _output = tendermint_light_client_membership::verify_membership(
            app_hash,
            vec![(kv_pair, proof)],
        );

        Ok(())
    }

    pub fn submit_misbehaviour(
        ctx: Context<SubmitMisbehaviour>,
        msg: MisbehaviourMsg,
    ) -> Result<()> {
        let client_data = &mut ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientAlreadyFrozen);

        let header_1: Header = borsh::BorshDeserialize::try_from_slice(&msg.header_1)
            .map_err(|_| error!(ErrorCode::InvalidHeader))?;
        let header_2: Header = borsh::BorshDeserialize::try_from_slice(&msg.header_2)
            .map_err(|_| error!(ErrorCode::InvalidHeader))?;

        let client_state: TmClientState = client_data.client_state.clone().into();

        let output = tendermint_light_client_misbehaviour::verify_misbehaviour(
            client_state,
            header_1,
            header_2,
        );

        // Freeze the client at the frozen height
        client_data.frozen = true;
        client_data.client_state.frozen_height = output.frozen_height.revision_height();

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
            client_data.frozen = true;
            client_data.client_state.frozen_height = client_data.client_state.latest_height;
        }

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
