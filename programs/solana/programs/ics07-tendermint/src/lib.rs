#![allow(unexpected_cfgs)]
#![allow(deprecated)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;

// FIXME:remove ed25519-consensus dep
use tendermint_light_client_update_client::solana::{SolanaClientState, SolanaUpdateClientInput, update_client};
use tendermint_light_client_membership::solana::{SolanaMembershipInput, SolanaKVPair, membership, create_membership_verification_request, create_non_membership_verification_request};
use tendermint_light_client_misbehaviour::solana::{SolanaMisbehaviourInput, check_for_misbehaviour};
use ibc_client_tendermint::types::{Header, ConsensusState as TmConsensusState};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_primitives::prelude::*;

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

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ConsensusState {
    pub timestamp: u64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
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

    pub fn update_client(
        ctx: Context<UpdateClient>,
        msg: UpdateClientMsg,
    ) -> Result<()> {
        let client_data = &mut ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientFrozen);

        let header: Header = borsh::BorshDeserialize::try_from_slice(&msg.client_message)
            .map_err(|_| error!(ErrorCode::InvalidHeader))?;

        let client_state = SolanaClientState {
            chain_id: client_data.client_state.chain_id.clone(),
            trust_level_numerator: client_data.client_state.trust_level_numerator,
            trust_level_denominator: client_data.client_state.trust_level_denominator,
            trusting_period: client_data.client_state.trusting_period as i64,
            unbonding_period: client_data.client_state.unbonding_period as i64,
            max_clock_drift: client_data.client_state.max_clock_drift,
            latest_height: client_data.client_state.latest_height,
            frozen_height: if client_data.frozen { Some(client_data.client_state.latest_height) } else { None },
        };

        let trusted_consensus_state = TmConsensusState {
            timestamp: ibc_primitives::Timestamp::from_nanoseconds(client_data.consensus_state.timestamp).unwrap(),
            root: client_data.consensus_state.root.to_vec().try_into().unwrap(),
            next_validators_hash: client_data.consensus_state.next_validators_hash.to_vec().try_into().unwrap(),
        };

        let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

        let input = SolanaUpdateClientInput {
            client_state,
            trusted_consensus_state,
            proposed_header: header.clone(),
            time: current_time,
        };

        let output = update_client(input);

        client_data.client_state.latest_height = output.new_client_state.latest_height;
        client_data.consensus_state.timestamp = output.new_consensus_state.timestamp.nanoseconds();
        client_data.consensus_state.root = output.new_consensus_state.root.as_bytes().try_into().unwrap();
        client_data.consensus_state.next_validators_hash = output.new_consensus_state.next_validators_hash.as_bytes().try_into().unwrap();

        Ok(())
    }

    pub fn verify_membership(
        ctx: Context<VerifyMembership>,
        msg: MembershipMsg,
    ) -> Result<()> {
        let client_data = &ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientFrozen);

        require!(msg.height <= client_data.client_state.latest_height, ErrorCode::InvalidHeight);

        let proof: MerkleProof = borsh::BorshDeserialize::try_from_slice(&msg.proof)
            .map_err(|_| error!(ErrorCode::InvalidProof))?;

        let kv_pair = SolanaKVPair {
            key: msg.path,
            value: msg.value,
        };

        let client_state = SolanaClientState {
            chain_id: client_data.client_state.chain_id.clone(),
            trust_level_numerator: client_data.client_state.trust_level_numerator,
            trust_level_denominator: client_data.client_state.trust_level_denominator,
            trusting_period: client_data.client_state.trusting_period as i64,
            unbonding_period: client_data.client_state.unbonding_period as i64,
            max_clock_drift: client_data.client_state.max_clock_drift,
            latest_height: client_data.client_state.latest_height,
            frozen_height: if client_data.frozen { Some(client_data.client_state.latest_height) } else { None },
        };

        let consensus_state = TmConsensusState {
            timestamp: ibc_primitives::Timestamp::from_nanoseconds(client_data.consensus_state.timestamp).unwrap(),
            root: client_data.consensus_state.root.to_vec().try_into().unwrap(),
            next_validators_hash: client_data.consensus_state.next_validators_hash.to_vec().try_into().unwrap(),
        };

        let request = create_membership_verification_request(kv_pair, proof);

        let input = SolanaMembershipInput {
            client_state,
            consensus_state,
            height: msg.height,
            delay_time_period: msg.delay_time_period,
            delay_block_period: msg.delay_block_period,
            request,
        };

        let output = membership(input);

        require!(output.success, ErrorCode::VerificationFailed);

        Ok(())
    }

    pub fn verify_non_membership(
        ctx: Context<VerifyNonMembership>,
        msg: MembershipMsg,
    ) -> Result<()> {
        let client_data = &ctx.accounts.client_data;

        require!(!client_data.frozen, ErrorCode::ClientFrozen);

        require!(msg.height <= client_data.client_state.latest_height, ErrorCode::InvalidHeight);
            .map_err(|_| error!(ErrorCode::InvalidProof))?;

        let kv_pair = SolanaKVPair {
            key: msg.path,
            value: vec![],
        };

        let client_state = SolanaClientState {
            chain_id: client_data.client_state.chain_id.clone(),
            trust_level_numerator: client_data.client_state.trust_level_numerator,
            trust_level_denominator: client_data.client_state.trust_level_denominator,
            trusting_period: client_data.client_state.trusting_period as i64,
            unbonding_period: client_data.client_state.unbonding_period as i64,
            max_clock_drift: client_data.client_state.max_clock_drift,
            latest_height: client_data.client_state.latest_height,
            frozen_height: if client_data.frozen { Some(client_data.client_state.latest_height) } else { None },
        };

        let consensus_state = TmConsensusState {
            timestamp: ibc_primitives::Timestamp::from_nanoseconds(client_data.consensus_state.timestamp).unwrap(),
            root: client_data.consensus_state.root.to_vec().try_into().unwrap(),
            next_validators_hash: client_data.consensus_state.next_validators_hash.to_vec().try_into().unwrap(),
        };

        let request = create_non_membership_verification_request(kv_pair, proof);

        let input = SolanaMembershipInput {
            client_state,
            consensus_state,
            height: msg.height,
            delay_time_period: msg.delay_time_period,
            delay_block_period: msg.delay_block_period,
            request,
        };

        let output = membership(input);

        require!(output.success, ErrorCode::VerificationFailed);

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

        let client_state = SolanaClientState {
            chain_id: client_data.client_state.chain_id.clone(),
            trust_level_numerator: client_data.client_state.trust_level_numerator,
            trust_level_denominator: client_data.client_state.trust_level_denominator,
            trusting_period: client_data.client_state.trusting_period as i64,
            unbonding_period: client_data.client_state.unbonding_period as i64,
            max_clock_drift: client_data.client_state.max_clock_drift,
            latest_height: client_data.client_state.latest_height,
            frozen_height: None,
        };

        let trusted_consensus_state = TmConsensusState {
            timestamp: ibc_primitives::Timestamp::from_nanoseconds(client_data.consensus_state.timestamp).unwrap(),
            root: client_data.consensus_state.root.to_vec().try_into().unwrap(),
            next_validators_hash: client_data.consensus_state.next_validators_hash.to_vec().try_into().unwrap(),
        };

        let misbehaviour = ibc_client_tendermint::types::Misbehaviour {
            client_id: msg.client_id.parse().map_err(|_| error!(ErrorCode::InvalidClientId))?,
            header1: header_1,
            header2: header_2,
        };

        let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

        let input = SolanaMisbehaviourInput {
            client_state,
            misbehaviour: &misbehaviour,
            trusted_consensus_state_1: trusted_consensus_state.clone(),
            trusted_consensus_state_2: trusted_consensus_state,
            time: current_time,
        };

        let output = check_for_misbehaviour(input);

        if output.misbehaviour_detected {
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
