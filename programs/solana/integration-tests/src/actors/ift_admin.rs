use super::Actor;
use crate::chain::Chain;
use crate::ift::TokenKind;
use solana_program_test::BanksClientError;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

/// Admin for the IFT program (checked via `IFTAppState.admin` field).
///
/// IFT uses its own admin field rather than the access manager's `ADMIN_ROLE`.
/// Operations like pause, admin transfer and admin-mint require the signer to
/// match `app_state.admin`.
///
/// In tests, typically constructed via
/// `IftAdmin::from_keypair(admin.keypair().insecure_clone())`
/// since the AM admin's pubkey is set as the initial IFT admin during init.
pub struct IftAdmin {
    keypair: Keypair,
}

impl Default for IftAdmin {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for IftAdmin {
    fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }
}

impl IftAdmin {
    pub fn new() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    pub const fn from_keypair(keypair: Keypair) -> Self {
        Self { keypair }
    }

    pub async fn set_paused(
        &self,
        chain: &mut Chain,
        paused: bool,
    ) -> Result<(), BanksClientError> {
        let ix = crate::ift::build_set_paused_ix(self.pubkey(), paused);
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn propose_admin(
        &self,
        chain: &mut Chain,
        new_admin: Pubkey,
    ) -> Result<(), BanksClientError> {
        let ix = crate::ift::build_propose_admin_ix(self.pubkey(), new_admin);
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn accept_admin(&self, chain: &mut Chain) -> Result<(), BanksClientError> {
        let ix = crate::ift::build_accept_admin_ix(self.pubkey());
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn cancel_admin_proposal(&self, chain: &mut Chain) -> Result<(), BanksClientError> {
        let ix = crate::ift::build_cancel_admin_proposal_ix(self.pubkey());
        super::send_tx(&self.keypair, chain, &[ix]).await
    }

    pub async fn admin_mint(
        &self,
        chain: &mut Chain,
        mint: Pubkey,
        receiver: Pubkey,
        amount: u64,
        token_kind: TokenKind,
    ) -> Result<(), BanksClientError> {
        let ix = crate::ift::build_admin_mint_ix(
            self.pubkey(),
            self.pubkey(),
            mint,
            receiver,
            amount,
            token_kind,
        );
        super::send_tx(&self.keypair, chain, &[ix]).await
    }
}
