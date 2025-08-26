use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

mod account_state;

use crate::adapter_client::AttestationAdapter;
use crate::cli::SolanaClientConfig;
use crate::AttestorError;
use ibc_eureka_solidity_types::msgs::IAttestorMsgs;

pub use account_state::AccountState;

/// Relevant chain peek options. For their Solana
/// interpretation see [these docs](https://docs.arbitrum.io/for-devs/troubleshooting-building#how-many-block-numbers-must-we-wait-for-in-arbitrum-before-we-can-confidently-state-that-the-transaction-has-reached-finality)
#[allow(dead_code)]
enum PeekKind {
    /// Latest L2 block
    Latest,
}

pub struct SolanaClient {
    _client: RpcClient,
    _account_key: Pubkey,
}

impl SolanaClient {
    pub fn _from_config(_config: &SolanaClientConfig) -> Self {
        todo!()
    }

    #[allow(dead_code)]
    async fn get_account_info_by_slot_height(
        &self,
        _peek_kind: &PeekKind,
    ) -> Result<AccountState, AttestorError> {
        todo!()
    }
}

impl AttestationAdapter for SolanaClient {
    async fn get_unsigned_state_attestation_at_height(
        &self,
        _height: u64,
    ) -> Result<IAttestorMsgs::StateAttestation, AttestorError> {
        todo!()
    }
    async fn get_unsigned_packet_attestation_at_height(
        &self,
        _packets: &[Vec<u8>],
        _height: u64,
    ) -> Result<IAttestorMsgs::PacketAttestation, AttestorError> {
        todo!()
    }
}
