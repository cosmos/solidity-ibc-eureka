//! Chunking operations for large payloads and proofs.

use anchor_lang::prelude::*;
use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use ibc_eureka_relayer_core::api::SolanaPacketTxs;
use solana_ibc_constants::CHUNK_DATA_SIZE;
use solana_ibc_types::{
    router::{router_instructions, MsgCleanupChunks, PayloadChunk, ProofChunk},
    MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket, MsgUploadChunk,
};

use crate::gmp;

impl super::TxBuilder {
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
        Ok(SolanaPacketTxs {
            chunks: chunk_txs,
            final_tx: recv_tx,
            cleanup_tx,
            gmp_result_pda: Vec::new(),
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

        let ack_tx = self.create_tx_bytes(&instructions)?;

        let cleanup_tx = self.build_packet_cleanup_tx(
            &msg.packet.source_client,
            msg.packet.sequence,
            &msg.payloads,
            msg.proof.total_chunks,
        )?;

        // TODO: Support multi-payload packets #602
        let gmp_result_pda = msg
            .packet
            .payloads
            .first()
            .and_then(|payload| {
                let gmp_program_id = self.resolve_port_program_id(&payload.source_port).ok()?;
                gmp::compute_gmp_result_pda(
                    &payload.source_port,
                    &msg.packet.source_client,
                    msg.packet.sequence,
                    gmp_program_id,
                )
            })
            .map(|pda| pda.to_bytes().to_vec())
            .unwrap_or_default();

        Ok(SolanaPacketTxs {
            chunks: chunk_txs,
            final_tx: ack_tx,
            cleanup_tx,
            gmp_result_pda,
        })
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

        // TODO: Support multi-payload packets #602
        let gmp_result_pda = msg
            .packet
            .payloads
            .first()
            .and_then(|payload| {
                let gmp_program_id = self.resolve_port_program_id(&payload.source_port).ok()?;
                gmp::compute_gmp_result_pda(
                    &payload.source_port,
                    &msg.packet.source_client,
                    msg.packet.sequence,
                    gmp_program_id,
                )
            })
            .map(|pda| pda.to_bytes().to_vec())
            .unwrap_or_default();

        Ok(SolanaPacketTxs {
            chunks: chunk_txs,
            final_tx: timeout_tx,
            cleanup_tx,
            gmp_result_pda,
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
