//! Packet instruction builders and chunking for recv, ack, and timeout packets.

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
    router::{
        router_instructions, Client, Commitment, IBCApp, IBCAppState, MsgCleanupChunks,
        PayloadChunk, ProofChunk, RouterState,
    },
    AccessManager, MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket, MsgUploadChunk,
};

use solana_ibc_types::attestation::{
    ClientState as AttestationClientState, ConsensusState as AttestationConsensusState,
};

use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use crate::{gmp, ift};

use super::derive_alt_address;

/// Result type for ALT transaction building: (`create_alt_tx`, `extend_alt_txs`, `packet_txs`)
type AltBuildResult = (Vec<u8>, Vec<u8>, Vec<Vec<u8>>);

/// Maximum accounts that fit in a Solana transaction without ALT
const MAX_ACCOUNTS_WITHOUT_ALT: usize = 20;

/// Batch size for ALT extension transactions
const ALT_EXTEND_BATCH_SIZE: usize = 20;

/// Derives client state and consensus state PDAs based on client type.
fn derive_light_client_pdas(
    client_id: &str,
    height: u64,
    light_client_program_id: Pubkey,
) -> Result<(Pubkey, Pubkey)> {
    match solana_ibc_constants::client_type_from_id(client_id) {
        Some(solana_ibc_constants::CLIENT_TYPE_ATTESTATION) => {
            let (cs, _) = AttestationClientState::pda(light_client_program_id);
            let (cons, _) = AttestationConsensusState::pda(height, light_client_program_id);
            Ok((cs, cons))
        }
        Some(solana_ibc_constants::CLIENT_TYPE_TENDERMINT) => {
            // Tendermint clients require chain_id for PDA derivation which is not
            // available in eth-to-solana relay. This should not occur in practice.
            anyhow::bail!(
                "Tendermint client type not supported for eth-to-solana relay (client: {client_id})"
            )
        }
        _ => {
            anyhow::bail!("Unknown client type for client ID: {client_id}")
        }
    }
}

/// Extracted payload info for recv packet processing.
struct RecvPayloadInfo<'a> {
    dest_port: &'a str,
    encoding: &'a str,
    value: &'a [u8],
}

/// Extract payload info from either `packet.payloads` or metadata + `payload_data`.
fn extract_recv_payload_info<'a>(
    msg: &'a MsgRecvPacket,
    payload_data: &'a [Vec<u8>],
) -> Result<RecvPayloadInfo<'a>> {
    if msg.packet.payloads.is_empty() {
        let [metadata] = msg.payloads.as_slice() else {
            anyhow::bail!("Expected exactly one recv packet payload metadata element");
        };
        let value = payload_data
            .first()
            .ok_or_else(|| anyhow::anyhow!("Missing payload data"))?
            .as_slice();
        Ok(RecvPayloadInfo {
            dest_port: &metadata.dest_port,
            encoding: &metadata.encoding,
            value,
        })
    } else {
        let [payload] = msg.packet.payloads.as_slice() else {
            anyhow::bail!("Expected exactly one recv packet payload element");
        };
        Ok(RecvPayloadInfo {
            dest_port: &payload.dest_port,
            encoding: &payload.encoding,
            value: &payload.value,
        })
    }
}

/// Extract `source_port` from either inline payloads or chunked metadata.
fn extract_source_port<'a>(
    packet_payloads: &'a [solana_ibc_types::Payload],
    metadata_payloads: &'a [solana_ibc_types::router::PayloadMetadata],
    context: &str,
) -> Result<&'a str> {
    if !packet_payloads.is_empty() {
        let [payload] = packet_payloads else {
            anyhow::bail!(
                "Expected exactly one {context} packet payload element, got {}",
                packet_payloads.len()
            );
        };
        Ok(&payload.source_port)
    } else if !metadata_payloads.is_empty() {
        let [payload_meta] = metadata_payloads else {
            anyhow::bail!(
                "Expected exactly one {context} packet payload metadata element, got {}",
                metadata_payloads.len()
            );
        };
        Ok(&payload_meta.source_port)
    } else {
        anyhow::bail!("No payload data found in either packet.payloads or payloads metadata");
    }
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

impl super::SolanaTxBuilder {
    fn build_recv_packet_instruction(
        &self,
        msg: &MsgRecvPacket,
        chunk_accounts: Vec<Pubkey>,
        payload_data: &[Vec<u8>],
        abi_info: Option<&super::payload_translator::AbiGmpAccountInfo>,
    ) -> Result<Instruction> {
        let payload_info = extract_recv_payload_info(msg, payload_data)?;

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app, _) = IBCApp::pda(payload_info.dest_port, self.solana_ics26_program_id);
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

        let light_client_program_id = self.resolve_client_program_id(&msg.packet.dest_client)?;
        let (client_state, consensus_state) = derive_light_client_pdas(
            &msg.packet.dest_client,
            msg.proof.height,
            light_client_program_id,
        )?;

        let ibc_app_program_id = self.resolve_port_program_id(payload_info.dest_port)?;
        let (ibc_app_state, _) = IBCAppState::pda(ibc_app_program_id);
        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(packet_receipt, false),
            AccountMeta::new(packet_ack, false),
            AccountMeta::new_readonly(ibc_app_program_id, false),
            AccountMeta::new(ibc_app_state, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(light_client_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];
        accounts.extend(
            chunk_accounts
                .into_iter()
                .map(|a| AccountMeta::new(a, false)),
        );

        if let Some(info) = abi_info {
            // For ABI payloads, use pre-extracted GMP accounts from packet data
            accounts.extend(info.gmp_accounts.clone());
        } else {
            // For protobuf payloads, extract GMP accounts normally
            let gmp_accounts = gmp::extract_gmp_accounts(
                payload_info.dest_port,
                payload_info.encoding,
                payload_info.value,
                &msg.packet.dest_client,
                ibc_app_program_id,
            )?;
            accounts.extend(gmp_accounts);
        }

        let mut data = router_instructions::recv_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    async fn build_ack_packet_instruction(
        &self,
        msg: &MsgAckPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        let source_port = extract_source_port(&msg.packet.payloads, &msg.payloads, "ack")?;

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app_pda, _) = IBCApp::pda(source_port, self.solana_ics26_program_id);

        let ibc_app_account = self
            .target_solana_client
            .get_account_with_commitment(&ibc_app_pda, CommitmentConfig::confirmed())
            .map_err(|e| anyhow::anyhow!("Failed to get IBC app account: {e}"))?
            .value
            .ok_or_else(|| anyhow::anyhow!("IBC app account not found"))?;

        if ibc_app_account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            anyhow::bail!("Account data too short for IBCApp account");
        }

        let mut account_data = &ibc_app_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut account_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;
        let ibc_app_program = ibc_app.app_program_id;

        let (app_state, _) = IBCAppState::pda(ibc_app_program);
        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (client, _) = Client::pda(&msg.packet.source_client, self.solana_ics26_program_id);

        let light_client_program_id = self.resolve_client_program_id(&msg.packet.source_client)?;
        let (client_state, consensus_state) = derive_light_client_pdas(
            &msg.packet.source_client,
            msg.proof.height,
            light_client_program_id,
        )?;

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(ibc_app_program, false),
            AccountMeta::new(app_state, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(light_client_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];
        accounts.extend(
            chunk_accounts
                .into_iter()
                .map(|a| AccountMeta::new(a, false)),
        );

        if let Some(result_pda) = gmp::find_gmp_result_pda(
            source_port,
            &msg.packet.source_client,
            msg.packet.sequence,
            ibc_app_program,
        ) {
            accounts.push(AccountMeta::new(result_pda, false));
        }

        let mut data = router_instructions::ack_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    fn build_timeout_packet_instruction(
        &self,
        msg: &MsgTimeoutPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        let source_port = extract_source_port(&msg.packet.payloads, &msg.payloads, "timeout")?;

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app, _) = IBCApp::pda(source_port, self.solana_ics26_program_id);
        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );

        let ibc_app_program_id = self.resolve_port_program_id(source_port)?;
        let (ibc_app_state, _) = IBCAppState::pda(ibc_app_program_id);
        let (client, _) = Client::pda(&msg.packet.source_client, self.solana_ics26_program_id);

        let light_client_program_id = self.resolve_client_program_id(&msg.packet.source_client)?;
        let (client_state, consensus_state) = derive_light_client_pdas(
            &msg.packet.source_client,
            msg.proof.height,
            light_client_program_id,
        )?;

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(ibc_app_program_id, false),
            AccountMeta::new(ibc_app_state, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(light_client_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];
        accounts.extend(
            chunk_accounts
                .into_iter()
                .map(|a| AccountMeta::new(a, false)),
        );

        if let Some(result_pda) = gmp::find_gmp_result_pda(
            source_port,
            &msg.packet.source_client,
            msg.packet.sequence,
            ibc_app_program_id,
        ) {
            accounts.push(AccountMeta::new(result_pda, false));
        }

        let mut data = router_instructions::timeout_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }
}

// ---------------------------------------------------------------------------
// Chunking and high-level packet builders
// ---------------------------------------------------------------------------

impl super::SolanaTxBuilder {
    fn split_into_chunks(data: &[u8]) -> Vec<Vec<u8>> {
        data.chunks(CHUNK_DATA_SIZE).map(<[u8]>::to_vec).collect()
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

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

        let (chunk_pda, _) = PayloadChunk::pda(
            self.fee_payer,
            client_id,
            sequence,
            payload_index,
            chunk_index,
            self.solana_ics26_program_id,
        );

        let accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
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
            payload_index: 0,
            chunk_index,
            chunk_data,
        };

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

        let (chunk_pda, _) = ProofChunk::pda(
            self.fee_payer,
            client_id,
            sequence,
            chunk_index,
            self.solana_ics26_program_id,
        );

        let accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
        ];

        let mut data = router_instructions::upload_proof_chunk_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    fn build_packet_chunk_txs(
        &self,
        client_id: &str,
        sequence: u64,
        msg_payloads: &[solana_ibc_types::router::PayloadMetadata],
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
        msg_payloads: &[solana_ibc_types::router::PayloadMetadata],
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

    fn build_packet_cleanup_tx(
        &self,
        client_id: &str,
        sequence: u64,
        msg_payloads: &[solana_ibc_types::router::PayloadMetadata],
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

    /// Derives the GMP result PDA bytes for a single-payload packet.
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

    /// Count unique accounts across all instructions.
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

    /// Check if account exists on-chain.
    fn account_exists(&self, pubkey: &Pubkey) -> bool {
        match self
            .target_solana_client
            .get_account_with_commitment(pubkey, CommitmentConfig::confirmed())
        {
            Ok(response) => response.value.is_some(),
            Err(e) => {
                tracing::warn!(%pubkey, error = %e, "RPC error checking account existence");
                false
            }
        }
    }

    /// Build transaction with ALT support.
    fn build_tx_with_alt(&self, instructions: &[Instruction]) -> Result<AltBuildResult> {
        let slot = self
            .target_solana_client
            .get_slot_with_commitment(CommitmentConfig::processed())
            .map_err(|e| anyhow::anyhow!("Failed to get slot for ALT: {e}"))?;

        let (alt_address, _) = derive_alt_address(slot, self.fee_payer);

        let mut alt_accounts: Vec<Pubkey> = Vec::new();
        let mut seen: HashSet<Pubkey> = HashSet::new();

        alt_accounts.push(self.fee_payer);
        seen.insert(self.fee_payer);
        alt_accounts.push(solana_sdk::system_program::id());
        seen.insert(solana_sdk::system_program::id());

        for ix in instructions {
            if seen.insert(ix.program_id) {
                alt_accounts.push(ix.program_id);
            }
            for acc in &ix.accounts {
                if seen.insert(acc.pubkey) {
                    if self.account_exists(&acc.pubkey) {
                        alt_accounts.push(acc.pubkey);
                    } else {
                        tracing::debug!("Excluding non-existent account {} from ALT", acc.pubkey);
                    }
                }
            }
        }

        tracing::debug!(
            "Building ALT: {} accounts, address={}, slot={}",
            alt_accounts.len(),
            alt_address,
            slot
        );

        let alt_create_tx = self.build_create_alt_tx(slot)?;

        let alt_extend_txs: Vec<Vec<u8>> = alt_accounts
            .chunks(ALT_EXTEND_BATCH_SIZE)
            .map(|batch| self.build_extend_alt_tx(slot, batch.to_vec()))
            .collect::<Result<Vec<_>>>()?;

        let final_tx = self.create_tx_bytes_with_alt(instructions, alt_address, alt_accounts)?;

        Ok((final_tx, alt_create_tx, alt_extend_txs))
    }

    /// Builds IFT `claim_refund` transaction for ack/timeout packets.
    ///
    /// Returns `None` if not applicable (non-IFT packet, no pending transfer).
    /// Tokens are safe in `PendingTransfer` and user can manually claim later.
    fn build_ift_claim_refund_tx(
        &self,
        payloads: &[solana_ibc_types::PayloadMetadata],
        payload_data: &[Vec<u8>],
        source_client: &str,
        sequence: u64,
    ) -> Option<Vec<u8>> {
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

    // -----------------------------------------------------------------------
    // High-level chunked packet builders (called from attested.rs)
    // -----------------------------------------------------------------------

    pub(crate) fn build_recv_packet_chunked(
        &self,
        msg: &MsgRecvPacket,
        payload_data: &[Vec<u8>],
        proof_data: &[u8],
        abi_info: Option<&super::payload_translator::AbiGmpAccountInfo>,
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
            msg,
            remaining_account_pubkeys,
            payload_data,
            abi_info,
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

        Ok(SolanaPacketTxs {
            chunks: chunk_txs,
            final_tx: recv_tx,
            cleanup_tx,
            alt_create_tx: vec![],
            alt_extend_txs: vec![],
            gmp_result_pda: Vec::new(),
            ift_finalize_transfer_tx: vec![],
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

        let unique_accounts = Self::count_unique_accounts(&instructions, self.fee_payer);

        tracing::debug!(
            "ack_packet: {} unique accounts (threshold {})",
            unique_accounts,
            MAX_ACCOUNTS_WITHOUT_ALT
        );

        let (ack_tx, alt_create_tx, alt_extend_txs) = if unique_accounts > MAX_ACCOUNTS_WITHOUT_ALT
        {
            tracing::debug!(
                "Using ALT: {} accounts exceeds {}",
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

        let ift_finalize_transfer_tx = self
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
            ift_finalize_transfer_tx,
        })
    }

    pub(crate) fn build_timeout_packet_chunked(
        &self,
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
            self.build_timeout_packet_instruction(msg, remaining_account_pubkeys)?;

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

        let ift_finalize_transfer_tx = self
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
            ift_finalize_transfer_tx,
        })
    }
}
