use crate::chain::Chain;
use crate::gmp::{self, GmpAckPacketParams, GmpRecvPacketParams, GmpTimeoutPacketParams};
use crate::router::{self, AckPacketParams, RecvPacketParams, RecvResult, TimeoutPacketParams};
use crate::Actor;
use solana_program_test::BanksClientError;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction};

pub struct Relayer {
    keypair: Keypair,
}

impl Default for Relayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for Relayer {
    fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }
}

impl Relayer {
    pub fn new() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    pub const fn keypair(&self) -> &Keypair {
        &self.keypair
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
        let (payload_ix, payload_pda) = router::build_upload_payload_chunk_ix(
            self.pubkey(),
            chain.client_id(),
            sequence,
            payload.to_vec(),
        );
        let (proof_ix, proof_pda) = router::build_upload_proof_chunk_ix(
            self.pubkey(),
            chain.client_id(),
            sequence,
            proof.to_vec(),
        );
        let tx = Transaction::new_signed_with_payer(
            &[payload_ix, proof_ix],
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
        Ok((payload_pda, proof_pda))
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
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await
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
            &chain.accounts,
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        let tx = Transaction::new_signed_with_payer(
            std::slice::from_ref(&result.ix),
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
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
            &chain.accounts,
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
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
            &chain.accounts,
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
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
            &chain.accounts,
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        let tx = Transaction::new_signed_with_payer(
            std::slice::from_ref(&result.ix),
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
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
            &chain.accounts,
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
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
            &chain.accounts,
            chain.client_id(),
            chain.counterparty_client_id(),
            chain.clock_time(),
            params,
        );
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
        Ok(commitment_pda)
    }
}
