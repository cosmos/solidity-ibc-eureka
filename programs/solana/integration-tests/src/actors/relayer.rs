//! Relayer actor.
//!
//! Uploads payload/proof chunks and submits `recv_packet`, `ack_packet`
//! and `timeout_packet` transactions across Router, GMP and IFT flows.
//! Must hold `RELAYER_ROLE` in the access manager.

use super::Actor;
use crate::chain::Chain;
use crate::gmp::{self, GmpAckPacketParams, GmpRecvPacketParams, GmpTimeoutPacketParams};
use crate::ift::{self, IftGmpAckPacketParams, IftGmpTimeoutPacketParams, TokenKind};
use crate::router::{self, AckPacketParams, RecvPacketParams, RecvResult, TimeoutPacketParams};
use solana_program_test::BanksClientError;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};

/// Relayer actor that uploads chunks and delivers IBC packets.
pub struct Relayer {
    keypair: Keypair,
}

impl Default for Relayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for Relayer {
    fn keypair(&self) -> &Keypair {
        &self.keypair
    }
}

impl Relayer {
    /// Create a relayer with a fresh random keypair.
    pub fn new() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    // ── Chunk upload ────────────────────────────────────────────────────

    /// Upload payload and proof chunks to the chain via real router instructions.
    pub async fn upload_chunks(
        &self,
        chain: &mut Chain,
        sequence: u64,
        payload: &[u8],
        proof: &[u8],
    ) -> Result<(Pubkey, Pubkey), BanksClientError> {
        let client_id = chain.client_id().to_string();
        self.upload_chunks_for_client(chain, &client_id, sequence, payload, proof)
            .await
    }

    /// Upload payload and proof chunks keyed to a specific `client_id`.
    ///
    /// Use this when the target client differs from the chain's primary client
    /// (e.g. multi-hop scenarios where Chain B routes through `"b-to-c"`).
    pub async fn upload_chunks_for_client(
        &self,
        chain: &mut Chain,
        client_id: &str,
        sequence: u64,
        payload: &[u8],
        proof: &[u8],
    ) -> Result<(Pubkey, Pubkey), BanksClientError> {
        let (payload_ix, payload_pda) = router::build_upload_payload_chunk_ix(
            self.pubkey(),
            client_id,
            sequence,
            payload.to_vec(),
        );
        let (proof_ix, proof_pda) =
            router::build_upload_proof_chunk_ix(self.pubkey(), client_id, sequence, proof.to_vec());
        self.send_tx(chain, &[payload_ix, proof_ix]).await?;
        Ok((payload_pda, proof_pda))
    }

    /// Upload 1 payload chunk and N proof chunks for multi-chunk proof delivery.
    pub async fn upload_chunks_with_multi_proof(
        &self,
        chain: &mut Chain,
        sequence: u64,
        payload: &[u8],
        proof_chunks: &[Vec<u8>],
    ) -> Result<(Pubkey, Vec<Pubkey>), BanksClientError> {
        let (payload_ix, payload_pda) = router::build_upload_payload_chunk_ix(
            self.pubkey(),
            chain.client_id(),
            sequence,
            payload.to_vec(),
        );

        let mut ixs = vec![payload_ix];
        let mut proof_pdas = Vec::with_capacity(proof_chunks.len());

        for (i, chunk_data) in proof_chunks.iter().enumerate() {
            let (ix, pda) = router::build_upload_proof_chunk_ix_at(
                self.pubkey(),
                chain.client_id(),
                sequence,
                i as u8,
                chunk_data.clone(),
            );
            ixs.push(ix);
            proof_pdas.push(pda);
        }

        self.send_tx(chain, &ixs).await?;
        Ok((payload_pda, proof_pdas))
    }

    /// Reclaim rent from consumed chunk accounts, closing them so they can
    /// be re-created if needed.
    pub async fn cleanup_chunks(
        &self,
        chain: &mut Chain,
        sequence: u64,
        payload_chunk_pda: Pubkey,
        proof_chunk_pda: Pubkey,
    ) -> Result<(), BanksClientError> {
        let ix = router::build_cleanup_chunks_ix(
            self.pubkey(),
            chain.client_id(),
            sequence,
            payload_chunk_pda,
            proof_chunk_pda,
        );
        self.send_tx(chain, &[ix]).await
    }

    // ── Router operations ───────────────────────────────────────────────

    /// Deliver a `recv_packet` to the destination chain.
    pub async fn recv_packet(
        &self,
        chain: &mut Chain,
        params: RecvPacketParams<'_>,
    ) -> Result<RecvResult, BanksClientError> {
        let result = router::build_recv_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, std::slice::from_ref(&result.ix))
            .await?;
        Ok(result)
    }

    /// Deliver an `ack_packet` back to the source chain.
    pub async fn ack_packet(
        &self,
        chain: &mut Chain,
        params: AckPacketParams<'_>,
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) = router::build_ack_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, &[ix]).await?;
        Ok(commitment_pda)
    }

    /// Deliver a `timeout_packet` back to the source chain.
    pub async fn timeout_packet(
        &self,
        chain: &mut Chain,
        params: TimeoutPacketParams<'_>,
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) = router::build_timeout_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, &[ix]).await?;
        Ok(commitment_pda)
    }

    /// Deliver several `recv_packet` instructions in a single transaction.
    ///
    /// Mirrors the e2e batching path where the relayer submits multiple
    /// router operations in one tx. Returns the per-packet `RecvResult`s in
    /// the same order the params were provided.
    pub async fn recv_packets_batched(
        &self,
        chain: &mut Chain,
        params_list: Vec<RecvPacketParams<'_>>,
    ) -> Result<Vec<RecvResult>, BanksClientError> {
        let results: Vec<RecvResult> = params_list
            .into_iter()
            .map(|params| {
                router::build_recv_packet_ix(
                    self.pubkey(),
                    chain.client_id(),
                    chain.counterparty_client_id(),
                    chain.clock_time(),
                    params,
                )
            })
            .collect();
        let ixs: Vec<_> = results.iter().map(|r| r.ix.clone()).collect();
        self.send_tx(chain, &ixs).await?;
        Ok(results)
    }

    /// Deliver several `ack_packet` instructions in a single transaction.
    ///
    /// Returns the per-packet commitment PDAs in the same order the params
    /// were provided.
    pub async fn ack_packets_batched(
        &self,
        chain: &mut Chain,
        params_list: Vec<AckPacketParams<'_>>,
    ) -> Result<Vec<Pubkey>, BanksClientError> {
        let built: Vec<_> = params_list
            .into_iter()
            .map(|params| {
                router::build_ack_packet_ix(
                    self.pubkey(),
                    chain.client_id(),
                    chain.counterparty_client_id(),
                    chain.clock_time(),
                    params,
                )
            })
            .collect();
        let ixs: Vec<_> = built.iter().map(|(ix, _)| ix.clone()).collect();
        let commitment_pdas: Vec<_> = built.iter().map(|(_, pda)| *pda).collect();
        self.send_tx(chain, &ixs).await?;
        Ok(commitment_pdas)
    }

    /// Deliver a `recv_packet` with multiple proof chunks.
    pub async fn recv_packet_multi_proof(
        &self,
        chain: &mut Chain,
        params: RecvPacketParams<'_>,
        proof_chunk_pdas: &[Pubkey],
    ) -> Result<RecvResult, BanksClientError> {
        let result = router::build_recv_packet_ix_multi_proof(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
            proof_chunk_pdas,
        );
        self.send_tx(chain, std::slice::from_ref(&result.ix))
            .await?;
        Ok(result)
    }

    /// Deliver an `ack_packet` with multiple proof chunks.
    pub async fn ack_packet_multi_proof(
        &self,
        chain: &mut Chain,
        params: AckPacketParams<'_>,
        proof_chunk_pdas: &[Pubkey],
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) = router::build_ack_packet_ix_multi_proof(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
            proof_chunk_pdas,
        );
        self.send_tx(chain, &[ix]).await?;
        Ok(commitment_pda)
    }

    // ── GMP operations ──────────────────────────────────────────────────

    /// Deliver a GMP `recv_packet` to the destination chain.
    pub async fn gmp_recv_packet(
        &self,
        chain: &mut Chain,
        params: GmpRecvPacketParams,
    ) -> Result<RecvResult, BanksClientError> {
        let result = gmp::build_gmp_recv_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, std::slice::from_ref(&result.ix))
            .await?;
        Ok(result)
    }

    /// Deliver a GMP `timeout_packet` back to the source chain.
    pub async fn gmp_timeout_packet(
        &self,
        chain: &mut Chain,
        params: GmpTimeoutPacketParams,
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) = gmp::build_gmp_timeout_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, &[ix]).await?;
        Ok(commitment_pda)
    }

    /// Deliver a GMP `ack_packet` back to the source chain.
    pub async fn gmp_ack_packet(
        &self,
        chain: &mut Chain,
        params: GmpAckPacketParams,
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) = gmp::build_gmp_ack_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, &[ix]).await?;
        Ok(commitment_pda)
    }

    // ── IFT GMP operations (ABI encoding) ────────────────────────────────

    /// Deliver a GMP `ack_packet` for an IFT transfer (ABI encoding).
    pub async fn ift_gmp_ack_packet(
        &self,
        chain: &mut Chain,
        params: IftGmpAckPacketParams,
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) = ift::build_ift_gmp_ack_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, &[ix]).await?;
        Ok(commitment_pda)
    }

    /// Deliver a GMP `timeout_packet` for an IFT transfer (ABI encoding).
    pub async fn ift_gmp_timeout_packet(
        &self,
        chain: &mut Chain,
        params: IftGmpTimeoutPacketParams,
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) = ift::build_ift_gmp_timeout_packet_ix(
            self.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        self.send_tx(chain, &[ix]).await?;
        Ok(commitment_pda)
    }

    /// Finalize an IFT transfer after the GMP result is available.
    pub async fn ift_finalize_transfer(
        &self,
        chain: &mut Chain,
        mint: Pubkey,
        sender: Pubkey,
        client_id: &str,
        sequence: u64,
        token_kind: TokenKind,
    ) -> Result<(), BanksClientError> {
        let ix = ift::build_finalize_transfer_ix(
            self.pubkey(),
            mint,
            sender,
            client_id,
            sequence,
            token_kind,
        );
        self.send_tx(chain, &[ix]).await
    }
}
