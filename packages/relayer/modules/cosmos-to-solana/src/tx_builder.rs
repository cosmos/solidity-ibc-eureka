//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to

//! Solana from events received from a Cosmos SDK chain.

use std::{collections::HashMap, sync::Arc};

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use ibc_client_tendermint::types::Header as TmHeader;
use ibc_eureka_relayer_lib::utils::solana::{convert_client_state_to_sol, MAX_CHUNK_SIZE};
use ibc_eureka_relayer_lib::{
    events::{
        solana::solana_timeout_packet_to_tm_timeout, EurekaEventWithHeight,
        SolanaEurekaEventWithHeight,
    },
    utils::{
        cosmos::{
            self, tm_create_client_params, tm_update_client_params, TmCreateClientParams,
            TmUpdateClientParams,
        },
        solana::{
            convert_consensus_state, ibc_to_solana_ack_packet, ibc_to_solana_recv_packet,
            target_events_to_timeout_msgs,
        },
    },
};
use ibc_proto_eureka::ibc::lightclients::tendermint::v1::Fraction;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::{
        instruction::{create_lookup_table, extend_lookup_table},
        state::AddressLookupTable,
        AddressLookupTableAccount,
    },
    commitment_config::CommitmentConfig,
    ed25519_instruction::{Ed25519SignatureOffsets, DATA_START},
    instruction::{AccountMeta, Instruction},
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    transaction::VersionedTransaction,
};
use tendermint::{chain::Id as ChainId, vote::CanonicalVote};
use tendermint_proto::Protobuf;

use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use crate::gmp;
use ibc_eureka_relayer_core::api;

use solana_ibc_types::{
    ics07::{ics07_instructions, ClientState, ConsensusState, SignatureData},
    router::{
        router_instructions, Client, ClientSequence, Commitment, IBCApp, IBCAppState, PayloadChunk,
        ProofChunk, RouterState,
    },
    MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket, MsgUploadChunk,
};
use tendermint_rpc::{Client as _, HttpClient};

/// Parameters for assembling timeout packet accounts
struct TimeoutAccountsParams {
    router_state: Pubkey,
    ibc_app: Pubkey,
    packet_commitment: Pubkey,
    ibc_app_program_id: Pubkey,
    ibc_app_state: Pubkey,
    client: Pubkey,
    client_state: Pubkey,
    consensus_state: Pubkey,
    fee_payer: Pubkey,
    router_program_id: Pubkey,
    light_client_program_id: Pubkey,
    chunk_accounts: Vec<Pubkey>,
}

/// Maximum compute units allowed per Solana transaction
const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

/// Priority fee in micro-lamports per compute unit
const DEFAULT_PRIORITY_FEE: u64 = 1000;

/// Parameters for uploading a header chunk (mirrors the Solana program's type)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
struct UploadChunkParams {
    chain_id: String,
    target_height: u64,
    chunk_index: u8,
    chunk_data: Vec<u8>,
}

/// Organized transactions for chunked packet operations
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PacketChunkedTxs {
    /// All chunk upload transactions for payloads and proof
    pub chunk_txs: Vec<Vec<u8>>,
    /// Final packet transaction (recv/ack/timeout)
    pub final_tx: Vec<u8>,
}

/// Helper to derive header chunk PDA
fn derive_header_chunk(
    submitter: Pubkey,
    chain_id: &str,
    height: u64,
    chunk_index: u8,
    program_id: Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            b"header_chunk",
            submitter.as_ref(),
            chain_id.as_bytes(),
            &height.to_le_bytes(),
            &[chunk_index],
        ],
        &program_id,
    )
}

/// Helper to derive ALT address from current slot and authority
fn derive_alt_address(slot: u64, authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[authority.as_ref(), &slot.to_le_bytes()],
        &solana_sdk::address_lookup_table::program::id(),
    )
}

/// The `TxBuilder` produces Solana transactions based on events from Cosmos SDK.
pub struct TxBuilder {
    /// The HTTP client for Cosmos chain.
    pub src_tm_client: HttpClient,
    /// The target Rpc Client for Solana.
    pub target_solana_client: Arc<RpcClient>,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: Pubkey,
    /// The Solana ICS07 program ID.
    pub solana_ics07_program_id: Pubkey,
    /// The fee payer address for transactions.
    pub fee_payer: Pubkey,
    /// Address Lookup Table address for reducing transaction size (optional).
    pub alt_address: Option<Pubkey>,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to parse program IDs
    pub const fn new(
        src_tm_client: HttpClient,
        target_solana_client: Arc<RpcClient>,
        solana_ics07_program_id: Pubkey,
        solana_ics26_program_id: Pubkey,
        fee_payer: Pubkey,
        alt_address: Option<Pubkey>,
    ) -> Result<Self> {
        Ok(Self {
            src_tm_client,
            target_solana_client,
            solana_ics26_program_id,
            solana_ics07_program_id,
            fee_payer,
            alt_address,
        })
    }

    async fn chain_id(&self) -> Result<String> {
        Ok(self
            .src_tm_client
            .latest_block()
            .await?
            .block
            .header
            .chain_id
            .into())
    }

    fn build_create_client_instruction(
        &self,
        chain_id: &str,
        latest_height: u64,
        client_state: &ClientState,
        consensus_state: &ConsensusState,
    ) -> Result<Instruction> {
        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (consensus_state_pda, _) = ConsensusState::pda(
            client_state_pda,
            latest_height,
            self.solana_ics07_program_id,
        );

        tracing::info!("Client state PDA: {}", client_state_pda);
        tracing::info!("Consensus state PDA: {}", consensus_state_pda);

        let accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(consensus_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = ics07_instructions::initialize_discriminator();

        let mut instruction_data = Vec::new();

        instruction_data.extend_from_slice(&discriminator);

        instruction_data.extend_from_slice(&chain_id.try_to_vec()?);
        instruction_data.extend_from_slice(&latest_height.try_to_vec()?);
        instruction_data.extend_from_slice(&client_state.try_to_vec()?);
        instruction_data.extend_from_slice(&consensus_state.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: instruction_data,
        })
    }

    fn build_recv_packet_instruction(
        &self,
        chain_id: &str,
        msg: &MsgRecvPacket,
        chunk_accounts: Vec<Pubkey>,
        payload_data: &[Vec<u8>],
    ) -> Result<Instruction> {
        // Validate exactly one payload element (inline or metadata for chunked)
        let dest_port = if msg.packet.payloads.is_empty() {
            let [metadata] = msg.payloads.as_slice() else {
                return Err(anyhow::anyhow!(
                    "Expected exactly one recv packet payload metadata element"
                ));
            };
            &metadata.dest_port
        } else {
            let [payload] = msg.packet.payloads.as_slice() else {
                return Err(anyhow::anyhow!(
                    "Expected exactly one recv packet payload element"
                ));
            };
            &payload.dest_port
        };

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app, _) = IBCApp::pda(dest_port, self.solana_ics26_program_id);

        let (client_sequence, _) =
            ClientSequence::pda(&msg.packet.dest_client, self.solana_ics26_program_id);
        let (packet_receipt, _) = Commitment::packet_receipt_pda(
            &msg.packet.dest_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (packet_ack, _) = Commitment::packet_ack_pda(
            &msg.packet.dest_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (client, _) = Client::pda(&msg.packet.dest_client, self.solana_ics26_program_id);

        let (client_state, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);

        let (consensus_state, _) =
            ConsensusState::pda(client_state, msg.proof.height, self.solana_ics07_program_id);

        // Resolve the actual IBC app program ID for this port
        let ibc_app_program_id = self.resolve_port_program_id(dest_port)?;

        let (ibc_app_state, _) = IBCAppState::pda(dest_port, ibc_app_program_id);

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(client_sequence, false),
            AccountMeta::new(packet_receipt, false),
            AccountMeta::new(packet_ack, false),
            AccountMeta::new_readonly(ibc_app_program_id, false), // IBC app program (e.g., ICS27 GMP)
            AccountMeta::new(ibc_app_state, false),               // IBC app state
            AccountMeta::new_readonly(self.solana_ics26_program_id, false), // router program
            AccountMeta::new(self.fee_payer, true),               // relayer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false), // instructions sysvar
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        // Chunk accounts must be first in remaining_accounts
        for chunk_account in chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

        // Extract GMP accounts (payload forwarded unmodified per IBC spec)
        let (dest_port_for_gmp, encoding, payload_value) = if msg.packet.payloads.is_empty() {
            // Chunked payload case
            let metadata = &msg.payloads[0];
            let data = &payload_data[0];
            (
                metadata.dest_port.as_str(),
                metadata.encoding.as_str(),
                data.as_slice(),
            )
        } else {
            // Inline payload case
            let payload = &msg.packet.payloads[0];
            (
                payload.dest_port.as_str(),
                payload.encoding.as_str(),
                payload.value.as_slice(),
            )
        };

        let gmp_accounts = gmp::extract_gmp_accounts(
            dest_port_for_gmp,
            encoding,
            payload_value,
            &msg.packet.dest_client,
            ibc_app_program_id,
        )?;
        accounts.extend(gmp_accounts);

        let mut data = router_instructions::recv_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    async fn build_ack_packet_instruction(
        &self,
        msg: &MsgAckPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        tracing::info!(
            "Building ack packet instruction for packet from {} to {}, sequence {}",
            msg.packet.source_client,
            msg.packet.dest_client,
            msg.packet.sequence
        );

        let solana_ics26_program_id = self.solana_ics26_program_id;

        let (router_state, _) = RouterState::pda(solana_ics26_program_id);

        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one ack packet payload element"
            ));
        };

        let source_port = payload.source_port.clone();

        let (ibc_app_pda, _) = IBCApp::pda(&source_port, solana_ics26_program_id);

        let ibc_app_account = self
            .target_solana_client
            .get_account_with_commitment(&ibc_app_pda, CommitmentConfig::confirmed())
            .map_err(|e| anyhow::anyhow!("Failed to get IBC app account: {e}"))?
            .value
            .ok_or_else(|| anyhow::anyhow!("IBC app account not found"))?;

        if ibc_app_account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for IBCApp account"));
        }

        let mut data = &ibc_app_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;

        let ibc_app_program = ibc_app.app_program_id;
        tracing::info!("IBC app program ID: {}", ibc_app_program);

        let (app_state, _) = IBCAppState::pda(&source_port, ibc_app_program);

        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        let (client, _) = Client::pda(&msg.packet.source_client, solana_ics26_program_id);
        tracing::info!(
            "Router client PDA for '{}': {}",
            msg.packet.source_client,
            client
        );

        let chain_id = self.chain_id().await?;
        tracing::info!("Cosmos chain ID for ICS07 derivation: {}", chain_id);

        let (client_state, _) = ClientState::pda(&chain_id, self.solana_ics07_program_id);
        tracing::info!("ICS07 client state PDA: {}", client_state);

        let (consensus_state, _) =
            ConsensusState::pda(client_state, msg.proof.height, self.solana_ics07_program_id);

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new(packet_commitment, false), // Will be closed after ack
            AccountMeta::new_readonly(ibc_app_program, false),
            AccountMeta::new(app_state, false),
            AccountMeta::new_readonly(self.solana_ics26_program_id, false),
            AccountMeta::new(self.fee_payer, true), // relayer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false), // instructions sysvar
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        for chunk_account in chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

        let mut data = router_instructions::ack_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    /// Build instruction for timeout packet
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
    fn build_timeout_packet_instruction(
        &self,
        chain_id: &str,
        msg: &MsgTimeoutPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        tracing::info!(
            "Building timeout packet instruction for packet from {} to {}, sequence {}",
            msg.packet.source_client,
            msg.packet.dest_client,
            msg.packet.sequence
        );

        let source_port = Self::extract_timeout_source_port(msg)?;
        let accounts = self.build_timeout_accounts_with_derived_keys(
            chain_id,
            msg,
            &source_port,
            chunk_accounts,
        )?;
        let data = Self::build_timeout_instruction_data(msg)?;

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    /// Extract source port from timeout packet message
    fn extract_timeout_source_port(msg: &MsgTimeoutPacket) -> Result<String> {
        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one timeout packet payload element"
            ));
        };
        Ok(payload.source_port.clone())
    }

    /// Derive PDAs, log derivation info, and build accounts list for timeout packet instruction
    #[allow(clippy::cognitive_complexity)]
    fn build_timeout_accounts_with_derived_keys(
        &self,
        chain_id: &str,
        msg: &MsgTimeoutPacket,
        source_port: &str,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Vec<AccountMeta>> {
        let program_id = self.solana_ics26_program_id;

        let (router_state, _) = RouterState::pda(program_id);
        let (ibc_app, _) = IBCApp::pda(source_port, program_id);
        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            program_id,
        );

        let ibc_app_program_id = self.resolve_port_program_id(source_port)?;
        let (ibc_app_state, _) = IBCAppState::pda(source_port, ibc_app_program_id);
        let (client, _) = Client::pda(&msg.packet.source_client, program_id);
        let (client_state, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (consensus_state, _) =
            ConsensusState::pda(client_state, msg.proof.height, self.solana_ics07_program_id);

        Ok(Self::assemble_timeout_accounts(TimeoutAccountsParams {
            router_state,
            ibc_app,
            packet_commitment,
            ibc_app_program_id,
            ibc_app_state,
            client,
            client_state,
            consensus_state,
            fee_payer: self.fee_payer,
            router_program_id: self.solana_ics26_program_id,
            light_client_program_id: self.solana_ics07_program_id,
            chunk_accounts,
        }))
    }

    /// Assemble timeout packet accounts vector
    fn assemble_timeout_accounts(params: TimeoutAccountsParams) -> Vec<AccountMeta> {
        let mut accounts = vec![
            AccountMeta::new_readonly(params.router_state, false),
            AccountMeta::new_readonly(params.ibc_app, false),
            AccountMeta::new(params.packet_commitment, false),
            AccountMeta::new_readonly(params.ibc_app_program_id, false),
            AccountMeta::new(params.ibc_app_state, false),
            AccountMeta::new_readonly(params.router_program_id, false),
            AccountMeta::new(params.fee_payer, true), // relayer
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false), // instructions sysvar
            AccountMeta::new_readonly(params.client, false),
            AccountMeta::new_readonly(params.light_client_program_id, false),
            AccountMeta::new_readonly(params.client_state, false),
            AccountMeta::new_readonly(params.consensus_state, false),
        ];

        for chunk_account in params.chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

        accounts
    }

    /// Build instruction data for timeout packet
    fn build_timeout_instruction_data(msg: &MsgTimeoutPacket) -> Result<Vec<u8>> {
        let mut data = router_instructions::timeout_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);
        Ok(data)
    }

    /// Fetch Cosmos client state from the light client on Solana.
    /// # Errors
    /// Returns an error if the client state cannot be fetched or decoded.
    fn cosmos_client_state(&self, chain_id: &str) -> Result<ClientState> {
        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&client_state_pda, CommitmentConfig::confirmed())
            .context("Failed to fetch client state account")?
            .value
            .ok_or_else(|| anyhow::anyhow!("Client state account not found"))?;

        let client_state = ClientState::try_from_slice(&account.data[ANCHOR_DISCRIMINATOR_SIZE..])
            .or_else(|_| {
                // If try_from_slice fails due to extra bytes, use deserialize which is more lenient
                let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
                ClientState::deserialize(&mut data)
            })
            .context("Failed to deserialize client state")?;

        Ok(client_state)
    }

    /// Helper function to split data into chunks
    fn split_into_chunks(data: &[u8]) -> Vec<Vec<u8>> {
        data.chunks(MAX_CHUNK_SIZE).map(<[u8]>::to_vec).collect()
    }

    /// Resolve the IBC app program ID for a given port
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch `IBCApp` account
    /// - Failed to deserialize account data
    fn resolve_port_program_id(&self, port_id: &str) -> Result<Pubkey> {
        let (ibc_app_account, _) = IBCApp::pda(port_id, self.solana_ics26_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&ibc_app_account, CommitmentConfig::confirmed())
            .map_err(|e| {
                anyhow::anyhow!("Failed to fetch IBCApp account for port '{port_id}': {e}")
            })?
            .value
            .ok_or_else(|| anyhow::anyhow!("IBCApp account not found for port '{port_id}'"))?;

        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for IBCApp account"));
        }

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;

        tracing::info!(
            "Resolved port '{}' to program ID: {}",
            port_id,
            ibc_app.app_program_id
        );

        Ok(ibc_app.app_program_id)
    }

    fn split_header_into_chunks(header_bytes: &[u8]) -> Vec<Vec<u8>> {
        Self::split_into_chunks(header_bytes)
    }

    /// Extracts signature data from Protobuf Tendermint Header for Ed25519 pre-verification
    ///
    /// This function:
    /// 1. Iterates through all commit signatures in the protobuf header
    /// 2. Matches validators by address (from `CommitSig`), not by index - order doesn't matter
    /// 3. Filters out absent signatures (`BlockIdFlagAbsent`)
    /// 4. For each present signature, builds the canonical vote sign bytes using protobuf encoding
    /// 5. Returns a vector of `SignatureData` containing (pubkey, msg, signature) tuples
    ///
    /// The sign bytes are constructed using Tendermint's canonical vote format:
    /// - Creates a `CanonicalVote` from the commit data (height, round, `block_id`, timestamp, `chain_id`)
    /// - Encodes as protobuf using length-delimited encoding (`MarshalDelimited`)
    ///
    /// Pre-verification on Solana: Each signature costs ~10k CU via `Ed25519Program` precompile,
    /// compared to ~30k CU for brine-ed25519 fallback verification
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to convert types
    /// - Failed to encode canonical vote as protobuf
    fn extract_signature_data_from_header(
        header: &ibc_client_tendermint::types::Header,
        chain_id: &str,
    ) -> Result<Vec<SignatureData>> {
        use tendermint::validator::Info as ValidatorInfo;

        let commit = &header.signed_header.commit;
        let validators = &header.validator_set.validators();

        // Parse chain ID
        let chain_id =
            ChainId::try_from(chain_id.to_string()).context("Failed to parse chain ID")?;

        let mut signature_data_vec = Vec::new();
        let mut seen_hashes = std::collections::HashSet::new();
        let mut duplicates_skipped = 0;

        // Iterate through commit signatures - match validators by address, not index
        for (idx, commit_sig) in commit.signatures.iter().enumerate() {
            // Extract signature data and validator address, skip if absent
            let (validator_address, timestamp, signature_opt) = match commit_sig {
                tendermint::block::CommitSig::BlockIdFlagCommit {
                    validator_address,
                    timestamp,
                    signature,
                }
                | tendermint::block::CommitSig::BlockIdFlagNil {
                    validator_address,
                    timestamp,
                    signature,
                } => (validator_address, timestamp, signature),
                tendermint::block::CommitSig::BlockIdFlagAbsent => continue,
            };

            let Some(signature_bytes) = signature_opt else {
                continue;
            };

            let validator: &ValidatorInfo = validators
                .iter()
                .find(|v| v.address == *validator_address)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Validator address {validator_address:?} not found in validator set",
                    )
                })?;

            let pubkey = match &validator.pub_key {
                tendermint::PublicKey::Ed25519(key) => key.as_bytes(),
                _ => {
                    anyhow::bail!("Only Ed25519 keys are supported for signature verification");
                }
            };

            let canonical_vote = CanonicalVote {
                vote_type: tendermint::vote::Type::Precommit,
                height: commit.height,
                round: commit.round,
                block_id: Some(commit.block_id),
                timestamp: Some(*timestamp),
                chain_id: chain_id.clone(),
            };

            // Encode as protobuf to get sign bytes (CometBFT uses MarshalDelimited = varint length-prefixed)
            let sign_bytes = <CanonicalVote as Protobuf<
                tendermint_proto::v0_38::types::CanonicalVote,
            >>::encode_length_delimited_vec(canonical_vote);

            let signature: [u8; 64] = signature_bytes
                .as_bytes()
                .try_into()
                .context("Signature must be 64 bytes")?;

            let signature_hash =
                solana_sdk::hash::hashv(&[pubkey, sign_bytes.as_slice(), &signature]).to_bytes();

            if !seen_hashes.insert(signature_hash) {
                duplicates_skipped += 1;
                tracing::info!(
                    "Skipping duplicate signature at index {} with hash {:?}",
                    idx,
                    &signature_hash[..8]
                );
                continue;
            }

            signature_data_vec.push(SignatureData {
                signature_hash,
                pubkey: pubkey.try_into().context("Public key must be 32 bytes")?,
                msg: sign_bytes,
                signature,
            });
        }

        tracing::info!(
            "Extracted {} signatures for pre-verification (out of {} total, {} duplicates skipped)",
            signature_data_vec.len(),
            commit.signatures.len(),
            duplicates_skipped
        );

        Ok(signature_data_vec)
    }

    /// Verify signatures off-chain for logging (returns all including invalid)
    fn verify_signatures_offchain(signature_data: Vec<SignatureData>) -> Vec<SignatureData> {
        use ed25519_consensus::{Signature, VerificationKey};

        let mut invalid_count = 0;
        for sig_data in &signature_data {
            let is_valid = VerificationKey::try_from(sig_data.pubkey.as_slice())
                .and_then(|pk| {
                    Signature::try_from(sig_data.signature.as_slice()).map(|sig| (pk, sig))
                })
                .is_ok_and(|(pk, sig)| pk.verify(&sig, &sig_data.msg).is_ok());

            if !is_valid {
                invalid_count += 1;
            }
        }

        if invalid_count > 0 {
            tracing::warn!(
                "{} invalid signatures (will still pre-verify)",
                invalid_count
            );
        }

        signature_data
    }

    /// Select minimal signatures to meet 2/3 threshold on `validator_set`
    /// and `trust_threshold` on `trusted_next_validator_set`
    fn select_minimal_signatures(
        signature_data: &[SignatureData],
        header: &TmHeader,
        trust_numerator: u64,
        trust_denominator: u64,
    ) -> Result<Vec<SignatureData>> {
        let untrusted_validator_set = &header.validator_set;
        let untrusted_total_power: u64 = untrusted_validator_set.total_voting_power().into();
        let untrusted_required_power = (untrusted_total_power * 2) / 3;

        let mut accumulated_power = 0u64;
        let mut selected = Vec::new();

        for (val_idx, validator) in untrusted_validator_set.validators().iter().enumerate() {
            let pubkey_bytes = validator.pub_key.to_bytes();

            if let Some(sig_data) = signature_data.iter().find(|sig| pubkey_bytes == sig.pubkey) {
                accumulated_power += validator.power();
                selected.push(sig_data.clone());

                if accumulated_power >= untrusted_required_power {
                    tracing::info!(
                        "Selected {} signatures reaching 2/3 at validator {}/{}",
                        selected.len(),
                        val_idx + 1,
                        untrusted_validator_set.validators().len()
                    );
                    break;
                }
            }
        }

        if accumulated_power < untrusted_required_power {
            anyhow::bail!(
                "Insufficient voting power: {accumulated_power} < {untrusted_required_power} required",
            );
        }

        // Verify selection meets trusted threshold
        let trusted_validator_set = &header.trusted_next_validator_set;
        let trusted_total_power: u64 = trusted_validator_set.total_voting_power().into();
        let trusted_required_power = (trusted_total_power * trust_numerator) / trust_denominator;

        let mut trusted_power = 0u64;
        let selected_pubkeys: std::collections::HashSet<_> =
            selected.iter().map(|s| s.pubkey.as_slice()).collect();

        for validator in trusted_validator_set.validators() {
            if selected_pubkeys.contains(validator.pub_key.to_bytes().as_slice()) {
                trusted_power += validator.power();
            }
        }

        if trusted_power < trusted_required_power {
            anyhow::bail!(
                "Selection fails trusted threshold: {trusted_power} < {trusted_required_power} required",
            );
        }

        Ok(selected)
    }

    /// Builds a single signature pre-verification transaction for Ed25519 signature verification
    ///
    /// Each transaction contains:
    /// 1. `Ed25519Program` instruction for signature verification
    /// 2. `pre_verify_signature` instruction that validates and stores result in PDA
    ///
    /// The signature verification PDA is derived using:
    /// `[b"sig_verify", hash(pubkey || msg || signature)]`
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to serialize signature data
    /// - Failed to build `Ed25519Program` instruction data
    /// - Failed to create transaction
    #[allow(deprecated)]
    fn build_pre_verify_signature_transaction(&self, sig_data: &SignatureData) -> Result<Vec<u8>> {
        // Build Ed25519Program instruction, we can't use ed25519_instruction constructor since we
        // don't have keypair now
        let mut instruction_data = vec![
            1u8, // number of signatures
            0u8, // padding
        ];

        // Ed25519 instruction format per Solana SDK: header, then pubkey, signature, message
        // See: https://github.com/solana-labs/solana/blob/master/sdk/src/ed25519_instruction.rs
        let data_start = u16::try_from(DATA_START).expect("DATA_START (16) must fit in u16");
        let pubkey_offset = data_start; // pubkey is first at DATA_START
        let signature_offset = data_start + 32; // signature after pubkey
        let message_data_offset = data_start + 32 + 64; // message after signature
        let message_data_size = u16::try_from(sig_data.msg.len())
            .context("CanonicalVote message exceeds 65,535 bytes (Ed25519 instruction limit)")?;

        // Create Ed25519SignatureOffsets struct (all data in current instruction, hence u16::MAX indices)
        let offsets = Ed25519SignatureOffsets {
            signature_offset,
            signature_instruction_index: u16::MAX,
            public_key_offset: pubkey_offset,
            public_key_instruction_index: u16::MAX,
            message_data_offset,
            message_data_size,
            message_instruction_index: u16::MAX,
        };

        // Serialize offsets struct to bytes (Pod type, C-compatible repr)
        // SAFETY: Ed25519SignatureOffsets is #[repr(C)] and implements Pod trait,
        // size is 14 bytes (7 u16 fields)
        #[allow(clippy::borrow_as_ptr)] // Required for Pod serialization
        let offsets_bytes =
            unsafe { std::slice::from_raw_parts((&raw const offsets).cast::<u8>(), 14) };
        instruction_data.extend_from_slice(offsets_bytes);

        // Append public key, signature, and message (in that order per Solana SDK)
        instruction_data.extend_from_slice(&sig_data.pubkey);
        instruction_data.extend_from_slice(&sig_data.signature);
        instruction_data.extend_from_slice(&sig_data.msg);

        let ed25519_ix = Instruction {
            program_id: solana_sdk::ed25519_program::ID,
            accounts: vec![],
            data: instruction_data,
        };

        let (sig_verify_pda, _) = Pubkey::find_program_address(
            &[b"sig_verify", &sig_data.signature_hash],
            &self.solana_ics07_program_id,
        );

        // Build pre_verify_signature instruction (singular)
        let accounts = vec![
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new(sig_verify_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        // Serialize single signature data
        let params_data = sig_data.try_to_vec()?;

        let mut data = ics07_instructions::pre_verify_signature_discriminator().to_vec();
        data.extend_from_slice(&params_data);

        let pre_verify_ix = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        // Create transaction with both instructions
        let tx_bytes = self.create_tx_bytes(&[ed25519_ix, pre_verify_ix])?;

        Ok(tx_bytes)
    }

    fn build_chunk_transactions(
        &self,
        chunks: &[Vec<u8>],
        chain_id: &str,
        target_height: u64,
    ) -> Result<Vec<Vec<u8>>> {
        let mut chunk_txs = Vec::new();

        for (index, chunk_data) in chunks.iter().enumerate() {
            let chunk_index = u8::try_from(index)
                .map_err(|_| anyhow::anyhow!("Chunk index {index} exceeds u8 max"))?;
            let upload_ix = self.build_upload_header_chunk_instruction(
                chain_id,
                target_height,
                chunk_index,
                chunk_data.clone(),
            )?;

            let chunk_tx = self.create_tx_bytes(&[upload_ix])?;

            chunk_txs.push(chunk_tx);
        }

        Ok(chunk_txs)
    }

    fn extend_compute_ix() -> Vec<Instruction> {
        let compute_budget_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
                MAX_COMPUTE_UNIT_LIMIT,
            );

        let priority_fee_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(
                DEFAULT_PRIORITY_FEE,
            );

        vec![compute_budget_ix, priority_fee_ix]
    }

    fn extend_compute_ix_with_heap() -> Vec<Instruction> {
        let compute_budget_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
                MAX_COMPUTE_UNIT_LIMIT,
            );

        let priority_fee_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(
                DEFAULT_PRIORITY_FEE,
            );

        let heap_size_ix =
            solana_sdk::compute_budget::ComputeBudgetInstruction::request_heap_frame(256 * 1024);

        vec![compute_budget_ix, priority_fee_ix, heap_size_ix]
    }

    fn build_upload_payload_chunk_instruction(
        &self,
        client_id: &str,
        sequence: u64,
        payload_index: u8,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index,
            chunk_index,
            chunk_data,
        };

        let (chunk_pda, _) = PayloadChunk::pda(
            self.fee_payer,
            client_id,
            sequence,
            payload_index,
            chunk_index,
            self.solana_ics26_program_id,
        );

        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let mut data = router_instructions::upload_payload_chunk_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    fn build_upload_proof_chunk_instruction(
        &self,
        client_id: &str,
        sequence: u64,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        let msg = MsgUploadChunk {
            client_id: client_id.to_string(),
            sequence,
            payload_index: 0, // Not used for proof chunks
            chunk_index,
            chunk_data,
        };

        let (chunk_pda, _) = ProofChunk::pda(
            self.fee_payer,
            client_id,
            sequence,
            chunk_index,
            self.solana_ics26_program_id,
        );

        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let mut data = router_instructions::upload_proof_chunk_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    fn build_upload_header_chunk_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        let params = UploadChunkParams {
            chain_id: chain_id.to_string(),
            target_height,
            chunk_index,
            chunk_data,
        };

        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (chunk_pda, _) = derive_header_chunk(
            self.fee_payer,
            chain_id,
            target_height,
            chunk_index,
            self.solana_ics07_program_id,
        );

        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new_readonly(client_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let mut data = ics07_instructions::upload_header_chunk_discriminator().to_vec();
        data.extend_from_slice(&params.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        })
    }

    /// Builds ALT creation transaction for the given slot and authority
    fn build_create_alt_tx(&self, slot: u64) -> Result<Vec<u8>> {
        let (create_ix, _alt_address) = create_lookup_table(
            self.fee_payer, // authority
            self.fee_payer, // payer
            slot,           // recent_slot
        );

        self.create_tx_bytes(&[create_ix])
    }

    /// Builds ALT extension transaction to add accounts to the ALT
    fn build_extend_alt_tx(&self, slot: u64, accounts: Vec<Pubkey>) -> Result<Vec<u8>> {
        let (alt_address, _) = derive_alt_address(slot, self.fee_payer);

        let extend_ix = extend_lookup_table(
            alt_address,          // lookup_table_address
            self.fee_payer,       // authority
            Some(self.fee_payer), // payer (optional)
            accounts,             // new_addresses
        );

        self.create_tx_bytes(&[extend_ix])
    }

    fn build_assemble_and_update_client_tx(
        &self,
        chain_id: &str,
        target_height: u64,
        trusted_height: u64,
        total_chunks: u8,
        signature_data: &[SignatureData],
        alt_config: Option<(u64, Vec<Pubkey>)>, // (slot, addresses)
    ) -> Result<Vec<u8>> {
        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (trusted_consensus_state, _) = ConsensusState::pda(
            client_state_pda,
            trusted_height,
            self.solana_ics07_program_id,
        );
        let (new_consensus_state, _) = ConsensusState::pda(
            client_state_pda,
            target_height,
            self.solana_ics07_program_id,
        );

        let mut accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new_readonly(trusted_consensus_state, false),
            AccountMeta::new(new_consensus_state, false),
            AccountMeta::new(self.fee_payer, true), // submitter
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        for chunk_index in 0..total_chunks {
            let (chunk_pda, _) = derive_header_chunk(
                self.fee_payer,
                chain_id,
                target_height,
                chunk_index,
                self.solana_ics07_program_id,
            );
            accounts.push(AccountMeta::new(chunk_pda, false));
        }

        // Add signature verification PDA accounts as remaining accounts
        for sig_data in signature_data {
            let (sig_verify_pda, _) = Pubkey::find_program_address(
                &[b"sig_verify", &sig_data.signature_hash],
                &self.solana_ics07_program_id,
            );

            accounts.push(AccountMeta::new_readonly(sig_verify_pda, false));
        }

        tracing::info!(
            "Assembly tx: {} chunks + {} pre-verified sigs",
            total_chunks,
            signature_data.len()
        );

        let mut data = ics07_instructions::assemble_and_update_client_discriminator().to_vec();

        let chain_id_len = u32::try_from(chain_id.len()).expect("chain_id too long");
        data.extend_from_slice(&chain_id_len.to_le_bytes());
        data.extend_from_slice(chain_id.as_bytes());
        data.extend_from_slice(&target_height.to_le_bytes());
        data.extend_from_slice(&[total_chunks]); // chunk_count parameter

        let ix = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        let mut instructions = Self::extend_compute_ix_with_heap();
        instructions.push(ix);

        // Use dedicated ALT if provided, otherwise fall back to static ALT
        match alt_config {
            Some((slot, addresses)) => {
                let (alt_address, _) = derive_alt_address(slot, self.fee_payer);
                self.create_tx_bytes_with_alt(&instructions, alt_address, addresses)
            }
            None => self.create_tx_bytes(&instructions),
        }
    }

    /// Create transaction bytes using provided ALT addresses
    /// Used when building transactions that reference an ALT that doesn't exist yet
    fn create_tx_bytes_with_alt(
        &self,
        instructions: &[Instruction],
        alt_address: Pubkey,
        alt_addresses: Vec<Pubkey>,
    ) -> Result<Vec<u8>> {
        if instructions.is_empty() {
            anyhow::bail!("No instructions to execute on Solana");
        }

        let recent_blockhash = self.get_recent_blockhash()?;

        // Build v0 message with the provided ALT
        let alt_account = AddressLookupTableAccount {
            key: alt_address,
            addresses: alt_addresses,
        };

        let v0_message =
            self.compile_v0_message_with_alt(instructions, recent_blockhash, alt_account)?;

        Self::serialize_v0_transaction(v0_message)
    }
}

impl TxBuilder {
    fn build_packet_chunk_txs(
        &self,
        client_id: &str,
        sequence: u64,
        msg_payloads: &[solana_ibc_types::PayloadMetadata],
        payload_data: &[Vec<u8>],
        proof_total_chunks: u8,
        proof_data: &[u8],
    ) -> Result<Vec<Vec<u8>>> {
        let mut chunk_txs = Vec::new();

        for (payload_idx, data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            if payload_idx < msg_payloads.len() && msg_payloads[payload_idx].total_chunks > 0 {
                let chunks = Self::split_into_chunks(data);
                for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                    let chunk_index = u8::try_from(chunk_idx)
                        .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                    let instruction = self.build_upload_payload_chunk_instruction(
                        client_id,
                        sequence,
                        payload_index,
                        chunk_index,
                        chunk_data.clone(),
                    )?;

                    chunk_txs.push(self.create_tx_bytes(&[instruction])?);
                }
            }
        }

        if proof_total_chunks > 0 {
            let chunks = Self::split_into_chunks(proof_data);
            for (chunk_idx, chunk_data) in chunks.iter().enumerate() {
                let chunk_index = u8::try_from(chunk_idx)
                    .map_err(|_| anyhow::anyhow!("Chunk index exceeds u8 max"))?;

                let instruction = self.build_upload_proof_chunk_instruction(
                    client_id,
                    sequence,
                    chunk_index,
                    chunk_data.clone(),
                )?;

                chunk_txs.push(self.create_tx_bytes(&[instruction])?);
            }
        }

        Ok(chunk_txs)
    }

    fn build_chunk_remaining_accounts(
        &self,
        client_id: &str,
        sequence: u64,
        msg_payloads: &[solana_ibc_types::PayloadMetadata],
        payload_data: &[Vec<u8>],
        proof_total_chunks: u8,
    ) -> Result<Vec<Pubkey>> {
        let mut remaining_account_pubkeys = Vec::new();

        for (payload_idx, _data) in payload_data.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            if payload_idx < msg_payloads.len() && msg_payloads[payload_idx].total_chunks > 0 {
                for chunk_idx in 0..msg_payloads[payload_idx].total_chunks {
                    let (chunk_pda, _) = PayloadChunk::pda(
                        self.fee_payer,
                        client_id,
                        sequence,
                        payload_index,
                        chunk_idx,
                        self.solana_ics26_program_id,
                    );
                    remaining_account_pubkeys.push(chunk_pda);
                }
            }
        }

        if proof_total_chunks > 0 {
            for chunk_idx in 0..proof_total_chunks {
                let (chunk_pda, _) = ProofChunk::pda(
                    self.fee_payer,
                    client_id,
                    sequence,
                    chunk_idx,
                    self.solana_ics26_program_id,
                );
                remaining_account_pubkeys.push(chunk_pda);
            }
        }

        Ok(remaining_account_pubkeys)
    }

    fn build_recv_packet_chunked(
        &self,
        chain_id: &str,
        msg: &MsgRecvPacket,
        payload_data: &[Vec<u8>],
        proof_data: &[u8],
    ) -> Result<PacketChunkedTxs> {
        let chunk_txs = self.build_packet_chunk_txs(
            &msg.packet.dest_client,
            msg.packet.sequence,
            &msg.payloads,
            payload_data,
            msg.proof.total_chunks,
            proof_data,
        )?;

        let remaining_account_pubkeys = self.build_chunk_remaining_accounts(
            &msg.packet.dest_client,
            msg.packet.sequence,
            &msg.payloads,
            payload_data,
            msg.proof.total_chunks,
        )?;

        let recv_instruction = self.build_recv_packet_instruction(
            chain_id,
            msg,
            remaining_account_pubkeys,
            payload_data,
        )?;

        let mut instructions = Self::extend_compute_ix();
        instructions.push(recv_instruction);

        let recv_tx = self.create_tx_bytes(&instructions)?;

        Ok(PacketChunkedTxs {
            chunk_txs,
            final_tx: recv_tx,
        })
    }

    async fn build_ack_packet_chunked(
        &self,
        msg: &MsgAckPacket,
        payload_data: &[Vec<u8>],
        proof_data: &[u8],
    ) -> Result<PacketChunkedTxs> {
        tracing::info!(
            "build_ack_packet_chunked: seq={}, payloads.len={}, proof.total_chunks={}",
            msg.packet.sequence,
            msg.payloads.len(),
            msg.proof.total_chunks
        );

        let chunk_txs = self.build_packet_chunk_txs(
            &msg.packet.source_client,
            msg.packet.sequence,
            &msg.payloads,
            payload_data,
            msg.proof.total_chunks,
            proof_data,
        )?;

        tracing::info!(
            "  Total chunk upload txs: {}, then 1 final ack_packet tx",
            chunk_txs.len()
        );

        let remaining_account_pubkeys = self.build_chunk_remaining_accounts(
            &msg.packet.source_client,
            msg.packet.sequence,
            &msg.payloads,
            payload_data,
            msg.proof.total_chunks,
        )?;

        tracing::info!(
            "  Adding {} remaining_accounts (chunk PDAs) to ack_packet instruction",
            remaining_account_pubkeys.len()
        );

        let ack_instruction = self
            .build_ack_packet_instruction(msg, remaining_account_pubkeys)
            .await?;

        let mut instructions = Self::extend_compute_ix();
        instructions.push(ack_instruction);

        let ack_tx = self.create_tx_bytes(&instructions)?;

        Ok(PacketChunkedTxs {
            chunk_txs,
            final_tx: ack_tx,
        })
    }

    fn build_timeout_packet_chunked(
        &self,
        chain_id: &str,
        msg: &MsgTimeoutPacket,
        payload_data: &[Vec<u8>],
        proof_data: &[u8],
    ) -> Result<PacketChunkedTxs> {
        let chunk_txs = self.build_packet_chunk_txs(
            &msg.packet.source_client,
            msg.packet.sequence,
            &msg.payloads,
            payload_data,
            msg.proof.total_chunks,
            proof_data,
        )?;

        let remaining_account_pubkeys = self.build_chunk_remaining_accounts(
            &msg.packet.source_client,
            msg.packet.sequence,
            &msg.payloads,
            payload_data,
            msg.proof.total_chunks,
        )?;

        let timeout_instruction =
            self.build_timeout_packet_instruction(chain_id, msg, remaining_account_pubkeys)?;

        let mut instructions = Self::extend_compute_ix();
        instructions.push(timeout_instruction);

        let timeout_tx = self.create_tx_bytes(&instructions)?;

        Ok(PacketChunkedTxs {
            chunk_txs,
            final_tx: timeout_tx,
        })
    }

    /// Validates that Solana client height is sufficient for proving events and returns proof parameters
    ///
    /// # Errors
    ///
    /// Returns an error if Solana client is not at a sufficient height to prove the events
    fn validate_height_and_get_proof_params(
        src_events: &[EurekaEventWithHeight],
        solana_latest_height: u64,
        solana_revision_number: u64,
    ) -> Result<ibc_proto_eureka::ibc::core::client::v1::Height> {
        let max_event_height = src_events
            .iter()
            .map(|e| e.height)
            .max()
            .unwrap_or_else(|| {
                let timeout_height = solana_latest_height.saturating_sub(1);
                tracing::debug!(
                    "Timeout proof: proving non-receipt at height {} using consensus state at {}",
                    timeout_height,
                    solana_latest_height
                );
                timeout_height
            });

        // Minimum height
        let required_height = max_event_height + 1;

        if solana_latest_height < required_height {
            anyhow::bail!(
                "Solana client is at height {solana_latest_height} but need height {required_height} to prove events at height {max_event_height}. Update Solana client to at least height {required_height} first!",
            );
        }

        let proof_height = ibc_proto_eureka::ibc::core::client::v1::Height {
            revision_number: solana_revision_number,
            revision_height: solana_latest_height,
        };

        tracing::debug!(
            target_height = proof_height.revision_height,
            max_event_height,
            "Using Solana's latest height for proof generation"
        );

        Ok(proof_height)
    }

    /// Updates a timeout message with proof data from a Tendermint timeout message
    ///
    /// # Errors
    ///
    /// Returns an error if the proof is too large to fit in u8
    fn update_timeout_proof_chunks(
        timeout_with_chunks: &mut ibc_eureka_relayer_lib::utils::solana::TimeoutPacketWithChunks,
        tm_msg: &ibc_proto_eureka::ibc::core::channel::v2::MsgTimeout,
    ) -> Result<()> {
        // Update proof chunks with actual proof data
        timeout_with_chunks
            .proof_chunks
            .clone_from(&tm_msg.proof_unreceived);

        // Update proof metadata
        let proof_total_chunks = u8::try_from(
            tm_msg
                .proof_unreceived
                .len()
                .div_ceil(MAX_CHUNK_SIZE)
                .max(1),
        )
        .context("proof too big to fit in u8")?;

        timeout_with_chunks.msg.proof.total_chunks = proof_total_chunks;

        // Update proof height from TM message (injected by inject_tendermint_proofs)
        if let Some(proof_height) = &tm_msg.proof_height {
            timeout_with_chunks.msg.proof.height = proof_height.revision_height;
            tracing::info!(
                "Updated timeout packet seq {} proof height to {} (from TM message)",
                timeout_with_chunks.msg.packet.sequence,
                proof_height.revision_height
            );
        }

        Ok(())
    }

    /// Build relay transaction from Cosmos events to Solana
    /// Returns a vector of packet transactions with chunks preserved
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to convert events to messages
    /// - Failed to build Solana instructions
    /// - Failed to create transaction bytes
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub async fn relay_events_chunked(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<SolanaEurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<PacketChunkedTxs>> {
        tracing::info!(
            "Relaying chunked events from Cosmos to Solana for client {}",
            dst_client_id
        );

        let chain_id = self.chain_id().await?;
        let solana_client_state = self.cosmos_client_state(&chain_id)?;
        let solana_latest_height = solana_client_state.latest_height.revision_height;

        tracing::debug!(
            chain_id = %chain_id,
            latest_height = solana_latest_height,
            "Solana client state retrieved"
        );

        let proof_height = Self::validate_height_and_get_proof_params(
            &src_events,
            solana_latest_height,
            solana_client_state.latest_height.revision_number,
        )?;

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let slot = self
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::finalized())
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        let timeout_msgs = target_events_to_timeout_msgs(
            dest_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            slot,
            now_since_unix.as_secs(),
        );

        // we don't care about signer address as no cosmos tx will be sent here
        let mock_signer_address = String::new();

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &mock_signer_address,
            now_since_unix.as_secs(),
        );

        tracing::info!("Events to relay to Solana:");
        tracing::info!("  - Timeout messages: {}", timeout_msgs.len());
        tracing::info!("  - Recv messages: {}", recv_msgs.len());
        tracing::info!("  - Ack messages: {}", ack_msgs.len());

        // convert to tm events so we can inject proofs
        let mut timeout_msgs_tm: Vec<_> = timeout_msgs
            .iter()
            .map(|timeout_with_chunks| {
                solana_timeout_packet_to_tm_timeout(
                    timeout_with_chunks.msg.clone(),
                    mock_signer_address.clone(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        cosmos::inject_tendermint_proofs(
            &mut recv_msgs,
            &mut ack_msgs,
            &mut timeout_msgs_tm,
            &self.src_tm_client,
            &proof_height,
        )
        .await?;

        // Convert back to Solana format and update proof data
        let mut timeout_msgs_with_chunks = timeout_msgs;
        for (idx, timeout_with_chunks) in timeout_msgs_with_chunks.iter_mut().enumerate() {
            let tm_msg = &timeout_msgs_tm[idx];
            Self::update_timeout_proof_chunks(timeout_with_chunks, tm_msg)?;
        }

        let mut packet_txs = Vec::new();
        let chain_id = self.chain_id().await?;

        // Process recv messages with chunking
        for recv_msg in recv_msgs {
            // Convert to Solana format with chunk data
            let recv_with_chunks = ibc_to_solana_recv_packet(recv_msg)?;

            // Build chunked transactions
            let chunked = self.build_recv_packet_chunked(
                &chain_id,
                &recv_with_chunks.msg,
                &recv_with_chunks.payload_chunks,
                &recv_with_chunks.proof_chunks,
            )?;

            packet_txs.push(chunked);
        }

        // Process ack messages with chunking
        for ack_msg in ack_msgs {
            // Convert to Solana format with chunk data
            let ack_with_chunks = ibc_to_solana_ack_packet(ack_msg)?;

            // Build chunked transactions
            let chunked = self
                .build_ack_packet_chunked(
                    &ack_with_chunks.msg,
                    &ack_with_chunks.payload_chunks,
                    &ack_with_chunks.proof_chunks,
                )
                .await?;

            packet_txs.push(chunked);
        }

        // Process timeout messages with chunking
        for timeout_with_chunks in timeout_msgs_with_chunks {
            // Build chunked transactions
            let chunked = self.build_timeout_packet_chunked(
                &chain_id,
                &timeout_with_chunks.msg,
                &timeout_with_chunks.payload_chunks,
                &timeout_with_chunks.proof_chunks,
            )?;

            packet_txs.push(chunked);
        }

        Ok(packet_txs)
    }

    fn create_tx_bytes(&self, instructions: &[Instruction]) -> Result<Vec<u8>> {
        if instructions.is_empty() {
            anyhow::bail!("No instructions to execute on Solana");
        }

        let recent_blockhash = self.get_recent_blockhash()?;

        let alt_addresses = match self.alt_address {
            Some(alt_address) => self.fetch_alt_addresses(alt_address)?,
            None => vec![],
        };

        self.create_v0_tx(instructions, recent_blockhash, alt_addresses)
    }

    fn get_recent_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.target_solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))
    }

    fn create_v0_tx(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
        alt_addresses: Vec<Pubkey>,
    ) -> Result<Vec<u8>> {
        let v0_message = if alt_addresses.is_empty() {
            self.compile_v0_message(instructions, recent_blockhash)?
        } else {
            let alt_account = AddressLookupTableAccount {
                key: self.alt_address.expect("ALT address should be set"),
                addresses: alt_addresses,
            };

            self.compile_v0_message_with_alt(instructions, recent_blockhash, alt_account)?
        };

        Self::serialize_v0_transaction(v0_message)
    }

    fn compile_v0_message(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<v0::Message> {
        v0::Message::try_compile(&self.fee_payer, instructions, &[], recent_blockhash)
            .map_err(|e| anyhow::anyhow!("Failed to compile v0 message: {e}"))
    }

    fn compile_v0_message_with_alt(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
        alt_account: AddressLookupTableAccount,
    ) -> Result<v0::Message> {
        v0::Message::try_compile(
            &self.fee_payer,
            instructions,
            &[alt_account],
            recent_blockhash,
        )
        .map_err(|e| anyhow::anyhow!("Failed to compile v0 message with ALT: {e}"))
    }

    fn serialize_v0_transaction(v0_message: v0::Message) -> Result<Vec<u8>> {
        let num_signatures = v0_message.header.num_required_signatures as usize;
        let versioned_tx = VersionedTransaction {
            signatures: vec![solana_sdk::signature::Signature::default(); num_signatures],
            message: VersionedMessage::V0(v0_message),
        };

        let serialized_tx = bincode::serialize(&versioned_tx)?;
        Ok(serialized_tx)
    }

    fn fetch_alt_addresses(&self, alt_address: Pubkey) -> Result<Vec<Pubkey>> {
        let alt_account = self
            .target_solana_client
            .get_account_with_commitment(&alt_address, CommitmentConfig::confirmed())
            .map_err(|e| anyhow::anyhow!("Failed to fetch ALT account {alt_address}: {e}"))?
            .value
            .ok_or_else(|| anyhow::anyhow!("ALT account {alt_address} not found"))?;

        let lookup_table = AddressLookupTable::deserialize(&alt_account.data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize ALT: {e}"))?;

        Ok(lookup_table.addresses.to_vec())
    }

    /// Create a new ICS07 Tendermint client on Solana
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch chain ID from Tendermint RPC
    /// - Failed to fetch client creation parameters (latest height, client state, consensus state)
    /// - Failed to convert Tendermint types to Solana types
    /// - Failed to serialize instruction data
    /// - Failed to get recent blockhash from Solana
    pub async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        // Parse trust level from parameters if provided (format: "1/16")
        let trust_level = if let Some(trust_level_str) = parameters.get("trust_level") {
            let parts: Vec<&str> = trust_level_str.split('/').collect();
            if parts.len() != 2 {
                anyhow::bail!(
                    "Invalid trust level format: expected 'numerator/denominator', got '{trust_level_str}'",
                );
            }
            let numerator = parts[0]
                .parse::<u64>()
                .map_err(|_| anyhow::anyhow!("Invalid trust level numerator: {}", parts[0]))?;
            let denominator = parts[1]
                .parse::<u64>()
                .map_err(|_| anyhow::anyhow!("Invalid trust level denominator: {}", parts[1]))?;

            if numerator == 0 || denominator == 0 {
                anyhow::bail!("Trust level numerator and denominator must be greater than 0");
            }
            if numerator >= denominator {
                anyhow::bail!("Trust level numerator must be less than denominator");
            }

            tracing::info!("Using custom trust level: {}/{}", numerator, denominator);
            Some(Fraction {
                numerator,
                denominator,
            })
        } else {
            None
        };

        let chain_id = self.chain_id().await?;
        let TmCreateClientParams {
            latest_height,
            client_state: tm_client_state,
            consensus_state: tm_consensus_state,
        } = tm_create_client_params(&self.src_tm_client, trust_level).await?;

        let client_state = convert_client_state_to_sol(tm_client_state)?;
        let consensus_state = convert_consensus_state(&tm_consensus_state)?;

        let instruction = self.build_create_client_instruction(
            &chain_id,
            latest_height,
            &client_state,
            &consensus_state,
        )?;

        self.create_tx_bytes(&[instruction])
    }

    /// Build chunked update client transactions to latest tendermint height
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Failed to fetch chain ID or client state from Solana
    /// - Header size requires more than 255 chunks (`u8::MAX`)
    /// - Failed to serialize header or instruction data
    /// - Failed to get recent blockhash from Solana
    /// - Chain ID string is too long for serialization
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub async fn update_client(&self, dst_client_id: String) -> Result<api::SolanaUpdateClient> {
        // ALT extension batch size: Each Pubkey is 32 bytes, so we batch ~20-25 accounts per transaction
        const ALT_EXTEND_BATCH_SIZE: usize = 20;

        let chain_id = self.chain_id().await?;
        let client_state = self.cosmos_client_state(&chain_id)?;

        let TmUpdateClientParams {
            target_height,
            trusted_height,
            proposed_header,
        } = tm_update_client_params(
            client_state.latest_height.revision_height,
            &self.src_tm_client,
            None,
        )
        .await?;

        tracing::info!(
            "Building chunked update client transactions for client {dst_client_id} to height {target_height}",
        );

        // Convert protobuf Header to ibc-rs Header type
        let header = TmHeader::try_from(proposed_header)
            .context("Failed to convert protobuf Header to ibc-rs Header")?;

        // Extract signature data for pre-verification
        let mut signature_data = Self::extract_signature_data_from_header(&header, &chain_id)?;
        signature_data = Self::verify_signatures_offchain(signature_data);
        signature_data = Self::select_minimal_signatures(
            &signature_data,
            &header,
            client_state.trust_level_numerator,
            client_state.trust_level_denominator,
        )?;

        let borsh_header = crate::borsh_conversions::header_to_borsh(header);
        let header_bytes = borsh_header.try_to_vec()?;

        let chunks = Self::split_header_into_chunks(&header_bytes);
        let total_chunks = u8::try_from(chunks.len())
            .map_err(|_| anyhow::anyhow!("Too many chunks: {} should fit u8", chunks.len()))?;

        tracing::info!(
            "Header size: {} bytes, split into {} header chunks",
            header_bytes.len(),
            total_chunks
        );

        // Build all preparatory transactions: signature pre-verification first, then header chunks
        let mut prep_txs = Vec::new();

        // Build and add pre-verify signature transactions first (one per signature)
        // Each signature gets its own transaction with Ed25519Program + pre_verify_signature instructions
        // These must come before chunks so signatures are verified and stored in PDAs
        if !signature_data.is_empty() {
            let total_signatures = signature_data.len();

            for sig_data in &signature_data {
                let pre_verify_tx = self.build_pre_verify_signature_transaction(sig_data)?;
                prep_txs.push(pre_verify_tx);
            }

            tracing::info!(
                "Added {} pre-verify signature transactions",
                total_signatures
            );
        }

        // Build header chunk transactions and add them to prep_txs (all submitted in parallel)
        let chunk_txs = self.build_chunk_transactions(&chunks, &chain_id, target_height)?;
        prep_txs.extend(chunk_txs);

        // Get current slot for ALT derivation
        let slot = self
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::processed())?;

        tracing::info!("Current Solana slot: {}", slot);

        // Build ALT creation transaction
        let alt_create_tx = self.build_create_alt_tx(slot)?;

        // Collect all accounts that need to be in the ALT for assembly transaction
        let (client_state_pda, _) = ClientState::pda(&chain_id, self.solana_ics07_program_id);
        let (trusted_consensus_state, _) = ConsensusState::pda(
            client_state_pda,
            trusted_height,
            self.solana_ics07_program_id,
        );
        let (new_consensus_state, _) = ConsensusState::pda(
            client_state_pda,
            target_height,
            self.solana_ics07_program_id,
        );

        let mut alt_accounts = vec![
            client_state_pda,
            trusted_consensus_state,
            new_consensus_state,
            self.fee_payer,
            solana_sdk::system_program::id(),
        ];

        // Add all chunk account addresses
        for chunk_index in 0..total_chunks {
            let (chunk_pda, _) = derive_header_chunk(
                self.fee_payer,
                &chain_id,
                target_height,
                chunk_index,
                self.solana_ics07_program_id,
            );
            alt_accounts.push(chunk_pda);
        }

        // Add signature verification PDA addresses
        for sig_data in &signature_data {
            let (sig_verify_pda, _) = Pubkey::find_program_address(
                &[b"sig_verify", &sig_data.signature_hash],
                &self.solana_ics07_program_id,
            );

            alt_accounts.push(sig_verify_pda);
        }

        tracing::info!(
            "ALT will contain {} signature accounts + {} chunk accounts = {} total accounts",
            signature_data.len(),
            total_chunks,
            alt_accounts.len()
        );

        // Build ALT extension transactions in batches to avoid transaction size limits
        let mut alt_extend_txs = Vec::new();

        for account_batch in alt_accounts.chunks(ALT_EXTEND_BATCH_SIZE) {
            let extend_tx = self.build_extend_alt_tx(slot, account_batch.to_vec())?;
            alt_extend_txs.push(extend_tx);
        }

        // Build assembly transaction that uses the new ALT
        let assembly_tx = self.build_assemble_and_update_client_tx(
            &chain_id,
            target_height,
            trusted_height,
            total_chunks,
            &signature_data,
            Some((slot, alt_accounts)),
        )?;

        let total_tx_count = 1 + alt_extend_txs.len() + prep_txs.len() + 1; // ALT create + extends + prep txs + assembly

        // Build cleanup transaction to reclaim rent from chunks and signatures
        let cleanup_tx =
            self.build_cleanup_tx(&chain_id, target_height, total_chunks, &signature_data)?;

        tracing::info!(
            "Built {} total transactions: 1 ALT create + {} ALT extends + {} prep txs ({} header chunks + {} signatures) + 1 assembly + 1 cleanup",
            total_tx_count + 1,
            alt_extend_txs.len(),
            prep_txs.len(),
            total_chunks,
            signature_data.len()
        );

        Ok(api::SolanaUpdateClient {
            chunk_txs: prep_txs,
            alt_create_tx,
            alt_extend_txs,
            assembly_tx,
            target_height,
            cleanup_tx,
        })
    }

    fn build_cleanup_tx(
        &self,
        chain_id: &str,
        target_height: u64,
        total_chunks: u8,
        signature_data: &[SignatureData],
    ) -> Result<Vec<u8>> {
        let mut accounts = vec![AccountMeta::new(self.fee_payer, true)];

        // Add all chunk accounts
        for chunk_index in 0..total_chunks {
            let (chunk_pda, _) = derive_header_chunk(
                self.fee_payer,
                chain_id,
                target_height,
                chunk_index,
                self.solana_ics07_program_id,
            );
            accounts.push(AccountMeta::new(chunk_pda, false));
        }

        // Add all signature verification accounts
        for sig_data in signature_data {
            let (sig_verify_pda, _) = Pubkey::find_program_address(
                &[b"sig_verify", &sig_data.signature_hash],
                &self.solana_ics07_program_id,
            );
            accounts.push(AccountMeta::new(sig_verify_pda, false));
        }

        let mut data = ics07_instructions::cleanup_incomplete_upload_discriminator().to_vec();
        data.extend_from_slice(&self.fee_payer.try_to_vec()?);

        let instruction = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        let mut instructions = Self::extend_compute_ix();
        instructions.push(instruction);

        self.create_tx_bytes(&instructions)
    }
}
