//! Attestation-based transaction builder for Eth-to-Solana relay.

use std::time::Duration;

use alloy::primitives::Address;
use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use ibc_eureka_relayer_lib::{
    aggregator::{Aggregator, Config as AggregatorConfig},
    events::{SolanaEurekaEventWithHeight, solana::SolanaEurekaEvent},
    utils::{
        cosmos as cosmos_utils,
        solana::{
            self as solana_utils, ibc_to_solana_ack_packet, ibc_to_solana_recv_packet,
            target_events_to_timeout_msgs,
        },
        solana_attested, wait_for_condition,
    },
};
use ibc_proto_eureka::ibc::core::{
    channel::v2::{MsgAcknowledgement, MsgRecvPacket},
    client::v1::Height,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use ibc_eureka_relayer_core::api::{self, SolanaPacketTxs};

use super::{RelayParams, SolanaTxBuilder};

/// Transaction builder using attestation proofs for Eth-to-Solana relay.
pub struct AttestedTxBuilder {
    aggregator: Aggregator,
    tx_builder: SolanaTxBuilder,
    ics26_eth_address: Address,
}

impl AttestedTxBuilder {
    /// Create a new [`AttestedTxBuilder`] instance.
    pub async fn new(
        aggregator_config: AggregatorConfig,
        tx_builder: SolanaTxBuilder,
        ics26_eth_address: Address,
    ) -> Result<Self> {
        let aggregator = Aggregator::from_config(aggregator_config).await?;
        Ok(Self {
            aggregator,
            tx_builder,
            ics26_eth_address,
        })
    }

    /// Get the inner `SolanaTxBuilder` reference.
    pub fn tx_builder(&self) -> &SolanaTxBuilder {
        &self.tx_builder
    }

    /// Get the ICS26 Ethereum address.
    pub const fn ics26_eth_address(&self) -> &Address {
        &self.ics26_eth_address
    }

    /// Relay events from Ethereum to Solana using attestations.
    pub async fn relay_events(
        &self,
        params: RelayParams,
    ) -> Result<(Vec<SolanaPacketTxs>, Option<api::SolanaUpdateClient>)> {
        tracing::info!(
            "Building attested relay: {} src events, {} target events",
            params.src_events.len(),
            params.dest_events.len()
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let (mut recv_msgs, mut ack_msgs) = cosmos_utils::src_events_to_recv_and_ack_msgs(
            params.src_events.clone(),
            &params.src_client_id,
            &params.dst_client_id,
            &params.src_packet_seqs,
            &params.dst_packet_seqs,
            "",
            now,
        );

        // Get Solana slot for timeout packet handling
        let slot = self
            .tx_builder
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::finalized())
            .map_err(|e| anyhow::anyhow!("Failed to get Solana slot: {e}"))?;

        // Extract max timeout timestamp before consuming dest_events
        let max_timeout_ts = max_timeout_timestamp(&params.dest_events);

        // Build timeout messages from destination events
        let timeout_msgs = target_events_to_timeout_msgs(
            params.dest_events,
            &params.src_client_id,
            &params.dst_client_id,
            &params.dst_packet_seqs,
            slot,
            now,
        );

        if recv_msgs.is_empty() && ack_msgs.is_empty() && timeout_msgs.is_empty() {
            tracing::info!("No packets to relay");
            return Ok((vec![], None));
        }

        // Resolve light client program ID for the destination client
        let light_client_program_id = self
            .tx_builder
            .resolve_client_program_id(&params.dst_client_id)?;

        // Get current attestation client state
        let client_state = self
            .tx_builder
            .attestation_client_state(&params.dst_client_id, light_client_program_id)?;
        let current_height = client_state.latest_height;

        let max_height = params
            .src_events
            .iter()
            .map(|e| e.height)
            .max()
            .unwrap_or(0);
        let required_height = max_height.saturating_add(1);

        let needs_height_update = current_height < required_height;

        // Check if timestamp update is needed for timeouts
        let consensus_ts = max_timeout_ts.and_then(|_| {
            self.tx_builder
                .attestation_consensus_state_timestamp_secs(
                    &params.dst_client_id,
                    current_height,
                    light_client_program_id,
                )
                .ok()
        });

        let needs_timestamp_update = matches!(
            (consensus_ts, max_timeout_ts),
            (Some(cs_ts), Some(timeout_ts)) if cs_ts < timeout_ts
        );

        let needs_update = needs_height_update || needs_timestamp_update;

        let proof_height = if needs_update {
            required_height.max(current_height.saturating_add(1))
        } else {
            current_height
        };

        tracing::info!(
            "Attestation client at height {}, required {}, needs_update: {}",
            current_height,
            required_height,
            needs_update
        );

        if needs_update {
            tracing::info!("Waiting for aggregator to finalize height {}", proof_height);

            wait_for_condition(
                Duration::from_secs(25 * 60),
                Duration::from_secs(1),
                || async {
                    let finalized_height = self.aggregator.get_latest_height().await?;
                    Ok(finalized_height >= proof_height)
                },
            )
            .await
            .context("Timeout waiting for aggregator to finalize height")?;

            tracing::info!("Aggregator has finalized height {}", proof_height);
        }

        let min_sigs = client_state.min_required_sigs as usize;

        let target_height = Height {
            revision_number: 0,
            revision_height: proof_height,
        };

        solana_attested::inject_solana_attestor_proofs(
            &self.aggregator,
            &mut recv_msgs,
            &mut ack_msgs,
            &target_height,
            min_sigs,
        )
        .await?;

        // Build update_client transaction only if needed
        let update_client = if needs_update {
            Some(
                self.build_attestation_update_client(&params.dst_client_id, proof_height)
                    .await?,
            )
        } else {
            None
        };

        let packets = self
            .build_packet_transactions(recv_msgs, ack_msgs, timeout_msgs)
            .await?;

        Ok((packets, update_client))
    }

    /// Build update_client transaction for attestation light client.
    async fn build_attestation_update_client(
        &self,
        dst_client_id: &str,
        target_height: u64,
    ) -> Result<api::SolanaUpdateClient> {
        tracing::info!(
            "Building attestation update_client for height {}",
            target_height
        );

        let light_client_program_id = self.tx_builder.resolve_client_program_id(dst_client_id)?;
        let min_sigs = self
            .tx_builder
            .attestation_client_min_sigs(dst_client_id, light_client_program_id)?;

        let state_attestation = self.aggregator.get_state_attestation(target_height).await?;

        tracing::info!(
            "Aggregator returned {} signatures, attestation_data size: {} bytes",
            state_attestation.signatures.len(),
            state_attestation.attested_data.len()
        );

        let proof_bytes = solana_attested::build_solana_membership_proof(
            state_attestation.attested_data,
            state_attestation.signatures,
            min_sigs,
        );

        let update_tx = self.build_attestation_update_client_tx(
            dst_client_id,
            target_height,
            proof_bytes,
            light_client_program_id,
        )?;

        Ok(api::SolanaUpdateClient {
            chunk_txs: vec![],
            alt_create_tx: vec![],
            alt_extend_txs: vec![],
            assembly_tx: update_tx,
            target_height,
            cleanup_tx: vec![],
        })
    }

    /// Build update_client transaction bytes for attestation light client.
    fn build_attestation_update_client_tx(
        &self,
        client_id: &str,
        new_height: u64,
        proof: Vec<u8>,
        light_client_program_id: Pubkey,
    ) -> Result<Vec<u8>> {
        use sha2::{Digest, Sha256};
        use solana_ibc_types::attestation::{
            AppState as AttestationAppState, ClientState as AttestationClientState,
            ConsensusState as AttestationConsensusState,
        };

        let (client_state_pda, _) = AttestationClientState::pda(light_client_program_id);
        let (new_consensus_state_pda, _) =
            AttestationConsensusState::pda(new_height, light_client_program_id);
        let (app_state_pda, _) = AttestationAppState::pda(light_client_program_id);

        let access_manager_program_id = self.tx_builder.resolve_access_manager_program_id()?;
        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[solana_ibc_types::AccessManager::SEED],
            &access_manager_program_id,
        );

        let discriminator = {
            let mut hasher = Sha256::new();
            hasher.update(b"global:update_client");
            let result = hasher.finalize();
            <[u8; 8]>::try_from(&result[..8]).expect("sha256 output is at least 8 bytes")
        };

        #[derive(AnchorSerialize)]
        struct UpdateClientParams {
            proof: Vec<u8>,
        }

        let mut data = discriminator.to_vec();
        client_id
            .to_string()
            .serialize(&mut data)
            .context("Failed to serialize client_id")?;
        new_height
            .serialize(&mut data)
            .context("Failed to serialize new_height")?;
        UpdateClientParams { proof }
            .serialize(&mut data)
            .context("Failed to serialize UpdateClientParams")?;

        let instruction = solana_sdk::instruction::Instruction {
            program_id: light_client_program_id,
            accounts: vec![
                solana_sdk::instruction::AccountMeta::new(client_state_pda, false),
                solana_sdk::instruction::AccountMeta::new_readonly(app_state_pda, false),
                solana_sdk::instruction::AccountMeta::new_readonly(access_manager_pda, false),
                solana_sdk::instruction::AccountMeta::new_readonly(
                    solana_sdk::sysvar::instructions::ID,
                    false,
                ),
                solana_sdk::instruction::AccountMeta::new(new_consensus_state_pda, false),
                solana_sdk::instruction::AccountMeta::new(self.tx_builder.fee_payer, true),
                solana_sdk::instruction::AccountMeta::new_readonly(
                    solana_sdk::system_program::ID,
                    false,
                ),
            ],
            data,
        };

        let mut instructions = SolanaTxBuilder::extend_compute_ix();
        instructions.push(instruction);

        self.tx_builder.create_tx_bytes(&instructions)
    }

    /// Update the attestation light client to the latest height.
    pub async fn update_client(
        &self,
        dst_client_id: &str,
    ) -> Result<api::SolanaUpdateClient> {
        let latest_height = self.aggregator.get_latest_height().await?;
        tracing::info!(
            "Updating attestation client {} to latest height {}",
            dst_client_id,
            latest_height
        );
        self.build_attestation_update_client(dst_client_id, latest_height)
            .await
    }

    async fn build_packet_transactions(
        &self,
        recv_msgs: Vec<MsgRecvPacket>,
        ack_msgs: Vec<MsgAcknowledgement>,
        timeout_msgs: Vec<solana_utils::TimeoutPacketWithChunks>,
    ) -> Result<Vec<SolanaPacketTxs>> {
        let mut results = Vec::new();

        for msg in recv_msgs {
            // Build hint for ABI-encoded payloads (original payload stays intact)
            let abi_hint = self.tx_builder.build_abi_hint_if_needed(&msg)?;

            let recv_with_chunks = ibc_to_solana_recv_packet(msg)?;
            let mut packet_txs = self.tx_builder.build_recv_packet_chunked(
                &recv_with_chunks.msg,
                &recv_with_chunks.payload_chunks,
                &recv_with_chunks.proof_chunks,
                abi_hint.as_ref(),
            )?;

            // Prepend store_hint tx before chunk uploads for ABI payloads
            if let Some(hint) = &abi_hint {
                let mut hint_instructions = super::SolanaTxBuilder::extend_compute_ix();
                hint_instructions.push(hint.store_hint_instruction.clone());
                let hint_tx = self.tx_builder.create_tx_bytes(&hint_instructions)?;
                packet_txs.chunks.insert(0, hint_tx);
            }

            results.push(packet_txs);
        }

        for msg in ack_msgs {
            let ack_with_chunks = ibc_to_solana_ack_packet(msg)?;
            let packet_txs = self
                .tx_builder
                .build_ack_packet_chunked(
                    &ack_with_chunks.msg,
                    &ack_with_chunks.payload_chunks,
                    &ack_with_chunks.proof_chunks,
                )
                .await?;
            results.push(packet_txs);
        }

        for timeout in timeout_msgs {
            tracing::info!(
                "Building timeout packet for sequence {}",
                timeout.msg.packet.sequence
            );
            let packet_txs = self.tx_builder.build_timeout_packet_chunked(
                &timeout.msg,
                &timeout.payload_chunks,
                &timeout.proof_chunks,
            )?;
            results.push(packet_txs);
        }

        Ok(results)
    }
}

/// Extract maximum timeout timestamp from Solana destination events.
///
/// Looks at `SendPacket` events to find the maximum timeout timestamp,
/// which indicates the latest timeout we may need to prove against.
fn max_timeout_timestamp(events: &[SolanaEurekaEventWithHeight]) -> Option<u64> {
    events
        .iter()
        .filter_map(|e| match &e.event {
            SolanaEurekaEvent::SendPacket(send) => {
                u64::try_from(send.timeout_timestamp).ok()
            }
            SolanaEurekaEvent::WriteAcknowledgement(_) => None,
        })
        .max()
}
