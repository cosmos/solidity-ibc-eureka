//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! Solana from events received from a Cosmos SDK chain.

mod chunking;
mod client;
mod packets;
mod transaction;

use std::{collections::HashMap, sync::Arc};

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use ibc_client_tendermint::types::Header as TmHeader;
use ibc_eureka_relayer_lib::utils::solana::convert_client_state_to_sol;
use ibc_eureka_relayer_lib::{
    events::{
        solana::solana_timeout_packet_to_tm_timeout, EurekaEventWithHeight, SolanaEurekaEvent,
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
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use ibc_eureka_relayer_core::api::{self, SolanaPacketTxs};

use solana_ibc_types::ics07::{ClientState, ConsensusState};

pub use transaction::derive_alt_address;

/// Parameters for assembling timeout packet accounts
pub(crate) struct TimeoutAccountsParams {
    pub access_manager: Pubkey,
    pub router_state: Pubkey,
    pub ibc_app: Pubkey,
    pub packet_commitment: Pubkey,
    pub ibc_app_program_id: Pubkey,
    pub ibc_app_state: Pubkey,
    pub client: Pubkey,
    pub client_state: Pubkey,
    pub consensus_state: Pubkey,
    pub fee_payer: Pubkey,
    pub router_program_id: Pubkey,
    pub light_client_program_id: Pubkey,
    pub chunk_accounts: Vec<Pubkey>,
}

/// Maximum compute units allowed per Solana transaction
pub(crate) const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

/// Priority fee in micro-lamports per compute unit
pub(crate) const DEFAULT_PRIORITY_FEE: u64 = 1000;

/// Parameters for uploading a header chunk (mirrors the Solana program's type)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub(crate) struct UploadChunkParams {
    pub chain_id: String,
    pub target_height: u64,
    pub chunk_index: u8,
    pub chunk_data: Vec<u8>,
}

/// Helper to derive header chunk PDA
pub(crate) fn derive_header_chunk(
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

/// The `TxBuilder` produces Solana transactions based on events from Cosmos SDK.
pub struct TxBuilder {
    /// The HTTP client for Cosmos chain.
    pub src_tm_client: tendermint_rpc::HttpClient,
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
    /// This function cannot currently fail but returns `Result` for API consistency.
    pub const fn new(
        src_tm_client: tendermint_rpc::HttpClient,
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

    /// Create a new ICS07 Tendermint client on Solana
    ///
    /// # Errors
    /// Returns an error if parameters are invalid or Solana/Tendermint calls fail.
    pub async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
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

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let client_state = convert_client_state_to_sol(tm_client_state)?;
        let consensus_state = convert_consensus_state(&tm_consensus_state)?;

        let instruction = self.build_create_client_instruction(
            &chain_id,
            latest_height,
            &client_state,
            &consensus_state,
            access_manager_program_id,
        )?;

        self.create_tx_bytes(&[instruction])
    }

    /// Build chunked update client transactions to latest tendermint height
    ///
    /// # Errors
    /// Returns an error if Solana/Tendermint calls fail or transaction building fails.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub async fn update_client(&self, dst_client_id: String) -> Result<api::SolanaUpdateClient> {
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

        let header = TmHeader::try_from(proposed_header)
            .context("Failed to convert protobuf Header to ibc-rs Header")?;

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

        let mut prep_txs = Vec::new();

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

        let chunk_txs = self.build_chunk_transactions(&chunks, &chain_id, target_height)?;
        prep_txs.extend(chunk_txs);

        let slot = self
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::processed())?;

        tracing::info!("Current Solana slot: {}", slot);

        let alt_create_tx = self.build_create_alt_tx(slot)?;

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

        let mut alt_extend_txs = Vec::new();

        for account_batch in alt_accounts.chunks(ALT_EXTEND_BATCH_SIZE) {
            let extend_tx = self.build_extend_alt_tx(slot, account_batch.to_vec())?;
            alt_extend_txs.push(extend_tx);
        }

        let assembly_tx = self.build_assemble_and_update_client_tx(
            &chain_id,
            target_height,
            trusted_height,
            total_chunks,
            &signature_data,
            Some((slot, alt_accounts)),
        )?;

        let total_tx_count = 1 + alt_extend_txs.len() + prep_txs.len() + 1;

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

    /// Build relay transaction from Cosmos events to Solana
    ///
    /// # Errors
    /// Returns an error if proof generation or transaction building fails.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub async fn relay_events_chunked(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<SolanaEurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<SolanaPacketTxs>> {
        self.relay_events_chunked_internal(
            src_events,
            dest_events,
            src_client_id,
            dst_client_id,
            src_packet_seqs,
            dst_packet_seqs,
            None,
        )
        .await
    }

    /// Internal relay implementation with optional proof height override
    #[allow(
        clippy::too_many_lines,
        clippy::cognitive_complexity,
        clippy::too_many_arguments
    )]
    async fn relay_events_chunked_internal(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<SolanaEurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
        proof_height_override: Option<u64>,
    ) -> Result<Vec<SolanaPacketTxs>> {
        tracing::info!(
            "Relaying chunked events from Cosmos to Solana for client {}{}",
            dst_client_id,
            if proof_height_override.is_some() {
                " (with proof height override)"
            } else {
                ""
            }
        );

        let chain_id = self.chain_id().await?;
        let solana_client_state = self.cosmos_client_state(&chain_id)?;
        let solana_latest_height = solana_client_state.latest_height.revision_height;

        tracing::debug!(
            chain_id = %chain_id,
            latest_height = solana_latest_height,
            "Solana client state retrieved"
        );

        let proof_height = match proof_height_override {
            Some(override_height) => {
                tracing::info!(
                    "Using proof height override: {} (current client height: {})",
                    override_height,
                    solana_latest_height
                );
                ibc_proto_eureka::ibc::core::client::v1::Height {
                    revision_number: solana_client_state.latest_height.revision_number,
                    revision_height: override_height,
                }
            }
            None => Self::validate_height_and_get_proof_params(
                &src_events,
                solana_latest_height,
                solana_client_state.latest_height.revision_number,
            )?,
        };

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

        let mut timeout_msgs_with_chunks = timeout_msgs;
        for (idx, timeout_with_chunks) in timeout_msgs_with_chunks.iter_mut().enumerate() {
            let tm_msg = &timeout_msgs_tm[idx];
            Self::update_timeout_proof_chunks(timeout_with_chunks, tm_msg);
        }

        let mut packet_txs = Vec::new();
        let chain_id = self.chain_id().await?;

        for recv_msg in recv_msgs {
            let recv_with_chunks = ibc_to_solana_recv_packet(recv_msg)?;

            let chunked = self.build_recv_packet_chunked(
                &chain_id,
                &recv_with_chunks.msg,
                &recv_with_chunks.payload_chunks,
                &recv_with_chunks.proof_chunks,
            )?;

            packet_txs.push(chunked);
        }

        for ack_msg in ack_msgs {
            let ack_with_chunks = ibc_to_solana_ack_packet(ack_msg)?;

            let chunked = self
                .build_ack_packet_chunked(
                    &ack_with_chunks.msg,
                    &ack_with_chunks.payload_chunks,
                    &ack_with_chunks.proof_chunks,
                )
                .await?;

            packet_txs.push(chunked);
        }

        for timeout_with_chunks in timeout_msgs_with_chunks {
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

    /// Build relay transactions with optional update client (auto-generated when needed)
    ///
    /// # Errors
    /// Returns an error if update client generation or relay building fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn relay_events_with_update(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<SolanaEurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<(Vec<SolanaPacketTxs>, Option<api::SolanaUpdateClient>)> {
        tracing::info!(
            "Relaying events from Cosmos to Solana for client {} (with update check)",
            dst_client_id
        );

        let chain_id = self.chain_id().await?;
        let solana_client_state = self.cosmos_client_state(&chain_id)?;
        let current_height = solana_client_state.latest_height.revision_height;

        tracing::debug!(
            chain_id = %chain_id,
            current_height = current_height,
            "Solana client state retrieved"
        );

        let max_timeout_ts = Self::max_timeout_timestamp(&dest_events);
        let current_consensus_timestamp_secs = if max_timeout_ts.is_some() {
            self.get_consensus_state_timestamp_secs(&chain_id, current_height)
                .ok()
        } else {
            None
        };

        let (update_client, proof_height) = if let Some(required_height) = Self::needs_update_client(
            &src_events,
            current_height,
            current_consensus_timestamp_secs,
            max_timeout_ts,
        ) {
            tracing::info!(
                "Client update needed: current height {} < required height {}",
                current_height,
                required_height
            );

            let update = self
                .update_client(dst_client_id.clone())
                .await
                .context("Failed to generate update client transactions")?;

            let target = update.target_height;
            tracing::info!(
                "Update client transactions generated, target height: {}",
                target
            );

            (Some(update), target)
        } else {
            tracing::info!(
                "No client update needed, current height {} is sufficient",
                current_height
            );
            (None, current_height)
        };

        let packets = self
            .relay_events_chunked_internal(
                src_events,
                dest_events,
                src_client_id,
                dst_client_id,
                src_packet_seqs,
                dst_packet_seqs,
                Some(proof_height),
            )
            .await?;

        tracing::info!(
            "Built {} packet transactions, update_client: {}",
            packets.len(),
            update_client.is_some()
        );

        Ok((packets, update_client))
    }

    // Relay helper functions

    fn max_timeout_timestamp(dest_events: &[SolanaEurekaEventWithHeight]) -> Option<u64> {
        dest_events
            .iter()
            .filter_map(|e| match &e.event {
                SolanaEurekaEvent::SendPacket(event) => {
                    Some(u64::try_from(event.timeout_timestamp).unwrap_or_default())
                }
                SolanaEurekaEvent::WriteAcknowledgement(_) => None,
            })
            .max()
    }

    fn get_consensus_state_timestamp_secs(&self, chain_id: &str, height: u64) -> Result<u64> {
        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (consensus_state_pda, _) =
            ConsensusState::pda(client_state_pda, height, self.solana_ics07_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&consensus_state_pda, CommitmentConfig::confirmed())
            .context("Failed to fetch consensus state account")?
            .value
            .ok_or_else(|| anyhow::anyhow!("Consensus state account not found"))?;

        let consensus_state =
            ConsensusState::try_from_slice(&account.data[ANCHOR_DISCRIMINATOR_SIZE..])
                .or_else(|_| {
                    let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
                    ConsensusState::deserialize(&mut data)
                })
                .context("Failed to deserialize consensus state")?;

        Ok(consensus_state.timestamp / 1_000_000_000)
    }

    fn needs_update_client(
        src_events: &[EurekaEventWithHeight],
        current_height: u64,
        current_consensus_timestamp_secs: Option<u64>,
        max_timeout_ts: Option<u64>,
    ) -> Option<u64> {
        if let (Some(consensus_ts), Some(timeout_ts)) =
            (current_consensus_timestamp_secs, max_timeout_ts)
        {
            if consensus_ts < timeout_ts {
                tracing::info!(
                    "Client update needed for timeout: consensus_ts={} < timeout_ts={}",
                    consensus_ts,
                    timeout_ts
                );
                return Some(current_height + 1);
            }
        }

        let max_event_height = src_events
            .iter()
            .map(|e| e.height)
            .max()
            .unwrap_or_else(|| current_height.saturating_sub(1));

        let required_height = max_event_height + 1;

        if current_height < required_height {
            Some(required_height)
        } else {
            None
        }
    }

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

    #[allow(clippy::cast_possible_truncation)]
    fn update_timeout_proof_chunks(
        timeout_with_chunks: &mut ibc_eureka_relayer_lib::utils::solana::TimeoutPacketWithChunks,
        tm_msg: &ibc_proto_eureka::ibc::core::channel::v2::MsgTimeout,
    ) {
        use ibc_eureka_relayer_lib::utils::solana::MAX_CHUNK_SIZE;

        let proof_bytes = &tm_msg.proof_unreceived;
        let proof_total_chunks = proof_bytes.len().div_ceil(MAX_CHUNK_SIZE) as u8;

        timeout_with_chunks.msg.proof.height = tm_msg
            .proof_height
            .as_ref()
            .map_or(0, |h| h.revision_height);
        timeout_with_chunks.msg.proof.total_chunks = proof_total_chunks;
        timeout_with_chunks.proof_chunks.clone_from(proof_bytes);
    }
}
