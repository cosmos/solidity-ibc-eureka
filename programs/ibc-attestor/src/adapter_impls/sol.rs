use std::str::FromStr;

use attestor_packet_membership::Packets;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

mod account_state;

use crate::adapter_client::{
    AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation,
};
use crate::cli::SolanaClientConfig;
use crate::AttestorError;

pub use account_state::AccountState;

/// Relevant chain peek options. For their Solana
/// interpretation see [these docs](https://docs.arbitrum.io/for-devs/troubleshooting-building#how-many-block-numbers-must-we-wait-for-in-arbitrum-before-we-can-confidently-state-that-the-transaction-has-reached-finality)
enum PeekKind {
    /// Latest L2 block
    Latest,
}

pub struct SolanaClient {
    client: RpcClient,
    account_key: Pubkey,
}

impl SolanaClient {
    pub fn from_config(config: SolanaClientConfig) -> Self {
        let client = RpcClient::new(config.url);
        let account_key = Pubkey::from_str(&config.account_key).unwrap();
        Self {
            client,
            account_key,
        }
    }

    async fn get_account_info_by_slot_height(
        &self,
        peek_kind: &PeekKind,
    ) -> Result<AccountState, AttestorError> {
        todo!()
    }
}

impl AttestationAdapter for SolanaClient {
    async fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> Result<UnsignedStateAttestation, AttestorError> {
        todo!()
    }
    async fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &Packets,
        height: u64,
    ) -> Result<UnsignedPacketAttestation, AttestorError> {
        todo!()
    }
}
