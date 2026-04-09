use super::Actor;
use crate::chain::Chain;
use solana_program_test::BanksClientError;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

/// Admin for the access manager (checked via `ADMIN_ROLE` in the AM account).
///
/// ICS26 Router and ICS27 GMP delegate authorization to the access manager
/// program. Operations like propose/accept/cancel AM transfer require the
/// signer to hold `ADMIN_ROLE` on the relevant AM instance.
///
/// The admin is an independent keypair whose pubkey is passed to the AM
/// `initialize` instruction by the `Deployer`.
pub struct Admin {
    keypair: Keypair,
}

impl Default for Admin {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for Admin {
    fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }
}

impl Admin {
    pub fn new() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    pub const fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    // ── ICS26 Router AM transfer ────────────────────────────────────────

    pub async fn ics26_propose_am_transfer(
        &self,
        chain: &mut Chain,
        new_access_manager: Pubkey,
    ) -> Result<(), BanksClientError> {
        let ix =
            crate::router::build_ics26_propose_am_transfer_ix(self.pubkey(), new_access_manager);
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn ics26_accept_am_transfer(
        &self,
        chain: &mut Chain,
        new_am_program_id: Pubkey,
    ) -> Result<(), BanksClientError> {
        let ix = crate::router::build_ics26_accept_am_transfer_ix(self.pubkey(), new_am_program_id);
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn ics26_cancel_am_transfer(
        &self,
        chain: &mut Chain,
    ) -> Result<(), BanksClientError> {
        let ix = crate::router::build_ics26_cancel_am_transfer_ix(self.pubkey());
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    // ── GMP AM transfer ─────────────────────────────────────────────────

    pub async fn gmp_propose_am_transfer(
        &self,
        chain: &mut Chain,
        new_access_manager: Pubkey,
    ) -> Result<(), BanksClientError> {
        let ix = crate::gmp::build_gmp_propose_am_transfer_ix(self.pubkey(), new_access_manager);
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn gmp_accept_am_transfer(
        &self,
        chain: &mut Chain,
        new_am_program_id: Pubkey,
    ) -> Result<(), BanksClientError> {
        let ix = crate::gmp::build_gmp_accept_am_transfer_ix(self.pubkey(), new_am_program_id);
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn gmp_cancel_am_transfer(&self, chain: &mut Chain) -> Result<(), BanksClientError> {
        let ix = crate::gmp::build_gmp_cancel_am_transfer_ix(self.pubkey());
        super::send_tx(&self.keypair, chain, &[ix]).await
    }
}
