//! Chunking operations for large payloads and proofs.

use std::collections::HashSet;

use anchor_lang::prelude::*;
use anyhow::Result;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use ibc_eureka_relayer_core::api::SolanaPacketTxs;
use solana_ibc_constants::CHUNK_DATA_SIZE;
use solana_ibc_types::{
    router::{router_instructions, MsgCleanupChunks, PayloadChunk, ProofChunk},
    MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket, MsgUploadChunk,
};

use super::transaction::derive_alt_address;

use crate::{gmp, ift};

/// Result type for ALT transaction building: (`create_alt_tx`, `extend_alt_txs`, `packet_txs`)
type AltBuildResult = (Vec<u8>, Vec<u8>, Vec<Vec<u8>>);

/// Maximum accounts that fit in a Solana transaction without ALT
const MAX_ACCOUNTS_WITHOUT_ALT: usize = 20;

/// Batch size for ALT extension transactions
const ALT_EXTEND_BATCH_SIZE: usize = 20;

impl super::TxBuilder {
    /// Derives the GMP result PDA bytes for a single-payload packet.
    /// Returns empty vec for empty payloads, errors on multi-payload.
    fn derive_gmp_result_pda_bytes(
        &self,
        payloads: &[solana_ibc_types::PayloadMetadata],
        source_client: &str,
        sequence: u64,
    ) -> Result<Vec<u8>> {
        match payloads {
            [payload] => Ok(self
                .resolve_port_program_id(&payload.source_port)
                .inspect_err(|err| {
                    tracing::warn!(
                        err = ?err,
                        "Failed to resolve program id for port {}",
                        &payload.source_port
                    );
                })
                .ok()
                .and_then(|gmp_program_id| {
                    gmp::find_gmp_result_pda(
                        &payload.source_port,
                        source_client,
                        sequence,
                        gmp_program_id,
                    )
                    .map(|pda| pda.to_bytes().to_vec())
                })
                .unwrap_or_default()),
            [] => Ok(vec![]),
            _ => anyhow::bail!("Multi-payload is not yet supported"),
        }
    }

    /// Builds IFT `claim_refund` transaction for ack/timeout packets.
    ///
    /// Returns `None` if not applicable (non-IFT packet, no pending transfer).
    /// Logs warnings for unexpected failures but continues - tokens are safe in
    /// `PendingTransfer` and user can manually claim later.
    fn build_ift_claim_refund_tx(
        &self,
        payloads: &[solana_ibc_types::PayloadMetadata],
        payload_data: &[Vec<u8>],
        source_client: &str,
        sequence: u64,
    ) -> Option<Vec<u8>> {
        // Only single-payload packets supported for IFT
        let [payload] = payloads else {
            return None;
        };

        let [data] = payload_data else {
            return None;
        };

        let gmp_program_id = match self.resolve_port_program_id(&payload.source_port) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(
                    source_port = %payload.source_port,
                    error = ?e,
                    "IFT: Failed to resolve port program ID for claim_refund"
                );
                return None;
            }
        };

        let params = ift::ClaimRefundParams {
            source_port: &payload.source_port,
            encoding: &payload.encoding,
            payload_value: data,
            source_client,
            sequence,
            solana_client: &self.target_solana_client,
            gmp_program_id,
            fee_payer: self.fee_payer,
        };

        // build_claim_refund_instruction logs internally for unexpected failures
        let instruction = ift::build_claim_refund_instruction(&params)?;

        let mut instructions = Self::extend_compute_ix();
        instructions.push(instruction);

        match self.create_tx_bytes(&instructions) {
            Ok(tx_bytes) => Some(tx_bytes),
            Err(e) => {
                tracing::warn!(
                    source_client = %source_client,
                    sequence = sequence,
                    error = ?e,
                    "IFT: Failed to create claim_refund transaction"
                );
                None
            }
        }
    }

    /// Helper function to split data into chunks
    pub(crate) fn split_into_chunks(data: &[u8]) -> Vec<Vec<u8>> {
        data.chunks(CHUNK_DATA_SIZE).map(<[u8]>::to_vec).collect()
    }

    pub(crate) fn build_upload_payload_chunk_instruction(
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

    pub(crate) fn build_upload_proof_chunk_instruction(
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

    pub(crate) fn build_packet_chunk_txs(
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

    pub(crate) fn build_chunk_remaining_accounts(
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

    pub(crate) fn build_recv_packet_chunked(
        &self,
        chain_id: &str,
        msg: &MsgRecvPacket,
        payload_data: &[Vec<u8>],
        proof_data: &[u8],
    ) -> Result<SolanaPacketTxs> {
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

        let cleanup_tx = self.build_packet_cleanup_tx(
            &msg.packet.dest_client,
            msg.packet.sequence,
            &msg.payloads,
            msg.proof.total_chunks,
        )?;

        // recv_packet doesn't create GMP result PDA - that happens when ack/timeout comes back
        // No claim_refund needed for recv packets (only for ack/timeout on source chain)
        Ok(SolanaPacketTxs {
            chunks: chunk_txs,
            final_tx: recv_tx,
            cleanup_tx,
            alt_create_tx: vec![],
            alt_extend_txs: vec![],
            gmp_result_pda: Vec::new(),
            ift_claim_refund_tx: vec![],
        })
    }

    pub(crate) async fn build_ack_packet_chunked(
        &self,
        msg: &MsgAckPacket,
        payload_data: &[Vec<u8>],
        proof_data: &[u8],
    ) -> Result<SolanaPacketTxs> {
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

        let ack_instruction = self
            .build_ack_packet_instruction(msg, remaining_account_pubkeys)
            .await?;

        let mut instructions = Self::extend_compute_ix();
        instructions.push(ack_instruction);

        // Count unique accounts across all instructions
        let unique_accounts = Self::count_unique_accounts(&instructions, self.fee_payer);

        tracing::info!(
            "build_ack_packet_chunked: {} unique accounts (threshold: {})",
            unique_accounts,
            MAX_ACCOUNTS_WITHOUT_ALT
        );

        // Build final transaction with or without ALT based on account count
        let (ack_tx, alt_create_tx, alt_extend_txs) = if unique_accounts > MAX_ACCOUNTS_WITHOUT_ALT
        {
            tracing::info!(
                "Using ALT for ack_packet: {} accounts exceeds threshold of {}",
                unique_accounts,
                MAX_ACCOUNTS_WITHOUT_ALT
            );
            self.build_tx_with_alt(&instructions)?
        } else {
            (self.create_tx_bytes(&instructions)?, vec![], vec![])
        };

        let cleanup_tx = self.build_packet_cleanup_tx(
            &msg.packet.source_client,
            msg.packet.sequence,
            &msg.payloads,
            msg.proof.total_chunks,
        )?;

        let gmp_result_pda = self.derive_gmp_result_pda_bytes(
            &msg.payloads,
            &msg.packet.source_client,
            msg.packet.sequence,
        )?;

        let ift_claim_refund_tx = self
            .build_ift_claim_refund_tx(
                &msg.payloads,
                payload_data,
                &msg.packet.source_client,
                msg.packet.sequence,
            )
            .unwrap_or_default();

        Ok(SolanaPacketTxs {
            chunks: chunk_txs,
            final_tx: ack_tx,
            cleanup_tx,
            alt_create_tx,
            alt_extend_txs,
            gmp_result_pda,
            ift_claim_refund_tx,
        })
    }

    /// Count unique accounts across all instructions
    fn count_unique_accounts(instructions: &[Instruction], fee_payer: Pubkey) -> usize {
        let mut accounts: HashSet<Pubkey> = HashSet::new();
        accounts.insert(fee_payer);
        for ix in instructions {
            accounts.insert(ix.program_id);
            for acc in &ix.accounts {
                accounts.insert(acc.pubkey);
            }
        }
        accounts.len()
    }

    /// Build transaction with ALT support (following `update_client` pattern)
    fn build_tx_with_alt(&self, instructions: &[Instruction]) -> Result<AltBuildResult> {
        // Get current slot for ALT derivation
        let slot = self
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::processed())
            .map_err(|e| anyhow::anyhow!("Failed to get slot for ALT: {e}"))?;

        let (alt_address, _) = derive_alt_address(slot, self.fee_payer);

        // Collect all unique accounts for ALT (only accounts that exist on-chain)
        // Signing-only PDAs (like mint_authority) don't exist as accounts and must be excluded
        let mut alt_accounts: Vec<Pubkey> = Vec::new();
        let mut seen: HashSet<Pubkey> = HashSet::new();

        // Add fee payer and system program first (these always exist)
        alt_accounts.push(self.fee_payer);
        seen.insert(self.fee_payer);
        alt_accounts.push(solana_sdk::system_program::id());
        seen.insert(solana_sdk::system_program::id());

        // Add all accounts from instructions, but only if they exist on-chain
        for ix in instructions {
            if seen.insert(ix.program_id) {
                // Program IDs always exist
                alt_accounts.push(ix.program_id);
            }
            for acc in &ix.accounts {
                if seen.insert(acc.pubkey) {
                    // Check if account exists before adding to ALT
                    // Non-existent accounts (signing-only PDAs) will be referenced directly
                    if self.account_exists(&acc.pubkey) {
                        alt_accounts.push(acc.pubkey);
                    } else {
                        tracing::debug!(
                            "Excluding non-existent account {} from ALT (signing-only PDA)",
                            acc.pubkey
                        );
                    }
                }
            }
        }

        tracing::info!(
            "Building ALT with {} accounts at address {} (slot {})",
            alt_accounts.len(),
            alt_address,
            slot
        );

        // Build ALT creation transaction
        let alt_create_tx = self.build_create_alt_tx(slot)?;

        // Build ALT extension transactions in batches
        let alt_extend_txs: Vec<Vec<u8>> = alt_accounts
            .chunks(ALT_EXTEND_BATCH_SIZE)
            .map(|batch| self.build_extend_alt_tx(slot, batch.to_vec()))
            .collect::<Result<Vec<_>>>()?;

        // Build final transaction using ALT
        let final_tx = self.create_tx_bytes_with_alt(instructions, alt_address, alt_accounts)?;

        Ok((final_tx, alt_create_tx, alt_extend_txs))
    }

    pub(crate) fn build_timeout_packet_chunked(
        &self,
        chain_id: &str,
        msg: &MsgTimeoutPacket,
        payload_data: &[Vec<u8>],
        proof_data: &[u8],
    ) -> Result<SolanaPacketTxs> {
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

        let cleanup_tx = self.build_packet_cleanup_tx(
            &msg.packet.source_client,
            msg.packet.sequence,
            &msg.payloads,
            msg.proof.total_chunks,
        )?;

        let gmp_result_pda = self.derive_gmp_result_pda_bytes(
            &msg.payloads,
            &msg.packet.source_client,
            msg.packet.sequence,
        )?;

        let ift_claim_refund_tx = self
            .build_ift_claim_refund_tx(
                &msg.payloads,
                payload_data,
                &msg.packet.source_client,
                msg.packet.sequence,
            )
            .unwrap_or_default();

        Ok(SolanaPacketTxs {
            chunks: chunk_txs,
            final_tx: timeout_tx,
            cleanup_tx,
            alt_create_tx: vec![],
            alt_extend_txs: vec![],
            gmp_result_pda,
            ift_claim_refund_tx,
        })
    }

    pub(crate) fn build_packet_cleanup_tx(
        &self,
        client_id: &str,
        sequence: u64,
        msg_payloads: &[solana_ibc_types::PayloadMetadata],
        proof_total_chunks: u8,
    ) -> Result<Vec<u8>> {
        let mut accounts = vec![AccountMeta::new(self.fee_payer, true)];

        for (payload_idx, payload_metadata) in msg_payloads.iter().enumerate() {
            let payload_index = u8::try_from(payload_idx)
                .map_err(|_| anyhow::anyhow!("Payload index exceeds u8 max"))?;

            for chunk_index in 0..payload_metadata.total_chunks {
                let (chunk_pda, _) = PayloadChunk::pda(
                    self.fee_payer,
                    client_id,
                    sequence,
                    payload_index,
                    chunk_index,
                    self.solana_ics26_program_id,
                );
                accounts.push(AccountMeta::new(chunk_pda, false));
            }
        }

        for chunk_index in 0..proof_total_chunks {
            let (chunk_pda, _) = ProofChunk::pda(
                self.fee_payer,
                client_id,
                sequence,
                chunk_index,
                self.solana_ics26_program_id,
            );
            accounts.push(AccountMeta::new(chunk_pda, false));
        }

        let payload_chunks: Vec<u8> = msg_payloads.iter().map(|p| p.total_chunks).collect();
        let msg = MsgCleanupChunks {
            client_id: client_id.to_string(),
            sequence,
            payload_chunks,
            total_proof_chunks: proof_total_chunks,
        };

        let mut data = router_instructions::cleanup_chunks_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        let instruction = Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        };

        let mut instructions = Self::extend_compute_ix();
        instructions.push(instruction);

        self.create_tx_bytes(&instructions)
    }
}
