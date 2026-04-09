use super::Actor;
use crate::chain::Chain;
use crate::gmp::{self, GmpSendCallParams};
use crate::ift::{self, IftTransferParams, TokenKind};
use crate::router::{self, SendPacketParams, SendResult};
use solana_program_test::BanksClientError;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction};

pub struct User {
    keypair: Keypair,
}

impl Default for User {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for User {
    fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }
}

impl User {
    pub fn new() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    pub const fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Send a packet via `test_ibc_app` (user is the payer).
    pub async fn send_packet(
        &self,
        chain: &mut Chain,
        params: SendPacketParams<'_>,
    ) -> Result<SendResult, BanksClientError> {
        let result = router::build_send_packet_ix(
            self.pubkey(),
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

    /// Send a GMP call (user pays fees).
    pub async fn send_call(
        &self,
        chain: &mut Chain,
        params: GmpSendCallParams<'_>,
    ) -> Result<Pubkey, BanksClientError> {
        let (ix, commitment_pda) =
            gmp::build_gmp_send_call_ix(self.pubkey(), self.pubkey(), chain.client_id(), params);
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.pubkey()),
            &[&self.keypair],
            chain.blockhash(),
        );
        chain.process_transaction(tx).await?;
        Ok(commitment_pda)
    }

    /// Send an IFT transfer (user pays fees).
    pub async fn ift_transfer(
        &self,
        chain: &mut Chain,
        mint: Pubkey,
        token_kind: TokenKind,
        params: IftTransferParams,
    ) -> Result<ift::IftTransferResult, BanksClientError> {
        let result = ift::build_ift_transfer_ix(
            self.pubkey(),
            self.pubkey(),
            chain.client_id(),
            mint,
            token_kind,
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
}
