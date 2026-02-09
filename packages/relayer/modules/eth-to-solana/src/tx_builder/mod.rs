//! Transaction building for Eth-to-Solana relay.
//!
//! Supports attestation-based light client mode.

mod attested;
mod packets;

use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::{
        instruction::{create_lookup_table, extend_lookup_table},
        state::AddressLookupTable,
        AddressLookupTableAccount,
    },
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    transaction::VersionedTransaction,
};

use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use ibc_eureka_relayer_lib::events::{EurekaEventWithHeight, SolanaEurekaEventWithHeight};
use solana_ibc_types::router::{IBCApp, RouterState};

pub use attested::AttestedTxBuilder;

/// Parameters for relaying events between Ethereum and Solana.
pub struct RelayParams {
    /// Events from the source chain (Ethereum)
    pub src_events: Vec<EurekaEventWithHeight>,
    /// Events from the destination chain (Solana)
    pub dest_events: Vec<SolanaEurekaEventWithHeight>,
    /// Client ID on the source chain
    pub src_client_id: String,
    /// Client ID on the destination chain
    pub dst_client_id: String,
    /// Packet sequences from the source chain
    pub src_packet_seqs: Vec<u64>,
    /// Packet sequences from the destination chain
    pub dst_packet_seqs: Vec<u64>,
}

/// Maximum compute units allowed per Solana transaction.
pub(crate) const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

/// Priority fee in micro-lamports per compute unit.
pub(crate) const DEFAULT_PRIORITY_FEE: u64 = 1000;

/// Solana-side transaction builder.
///
/// Contains only the Solana RPC and program config needed for building
/// transactions, without any source-chain specific dependencies.
pub struct SolanaTxBuilder {
    /// The target Solana RPC client.
    pub target_solana_client: Arc<RpcClient>,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: Pubkey,
    /// The fee payer address for transactions.
    pub fee_payer: Pubkey,
    /// Address Lookup Table address for reducing transaction size.
    pub alt_address: Option<Pubkey>,
}

impl SolanaTxBuilder {
    /// Creates a new `SolanaTxBuilder`.
    pub fn new(
        target_solana_client: Arc<RpcClient>,
        solana_ics26_program_id: Pubkey,
        fee_payer: Pubkey,
        alt_address: Option<Pubkey>,
    ) -> Result<Self> {
        Ok(Self {
            target_solana_client,
            solana_ics26_program_id,
            fee_payer,
            alt_address,
        })
    }

    /// Resolves the access manager program ID from the router state.
    pub(crate) fn resolve_access_manager_program_id(&self) -> Result<Pubkey> {
        let (router_state_pda, _) = RouterState::pda(self.solana_ics26_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&router_state_pda, CommitmentConfig::confirmed())
            .map_err(|e| anyhow::anyhow!("Failed to fetch RouterState account: {e}"))?
            .value
            .ok_or_else(|| anyhow::anyhow!("Router state account not found"))?;

        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for RouterState"));
        }

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let router_state = solana_ibc_types::RouterState::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize RouterState: {e}"))?;

        Ok(router_state.access_manager)
    }

    /// Resolve the IBC app program ID for a given port.
    pub(crate) fn resolve_port_program_id(&self, port_id: &str) -> Result<Pubkey> {
        let (ibc_app_account, _) = IBCApp::pda(port_id, self.solana_ics26_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&ibc_app_account, CommitmentConfig::confirmed())
            .map_err(|e| {
                anyhow::anyhow!("Failed to fetch IBCApp account for port '{port_id}': {e}")
            })?
            .value
            .ok_or_else(|| anyhow::anyhow!("IBCApp account not found for port '{port_id}'"))?;

        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for IBCApp"));
        }

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp: {e}"))?;

        Ok(ibc_app.app_program_id)
    }

    /// Resolve the light client program ID.
    pub(crate) fn resolve_client_program_id(&self, client_id: &str) -> Result<Pubkey> {
        let (client_account, _) =
            solana_ibc_types::Client::pda(client_id, self.solana_ics26_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&client_account, CommitmentConfig::confirmed())
            .map_err(|e| {
                anyhow::anyhow!("Failed to fetch Client account for '{client_id}': {e}")
            })?
            .value
            .ok_or_else(|| anyhow::anyhow!("Client account not found for '{client_id}'"))?;

        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for Client"));
        }

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let client = solana_ibc_types::ClientAccount::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize Client: {e}"))?;

        Ok(client.client_program_id)
    }

    /// Fetch the attestation light client state from Solana.
    pub(crate) fn attestation_client_state(
        &self,
        client_id: &str,
        light_client_program_id: Pubkey,
    ) -> Result<solana_ibc_types::attestation::ClientState> {
        use solana_ibc_types::attestation::ClientState as AttestationClientState;

        let (client_state_pda, _) = AttestationClientState::pda(client_id, light_client_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&client_state_pda, CommitmentConfig::confirmed())
            .context("Failed to fetch attestation client state account")?
            .value
            .ok_or_else(|| anyhow::anyhow!("Attestation client state account not found"))?;

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let client_state = AttestationClientState::deserialize(&mut data)
            .context("Failed to deserialize attestation client state")?;

        Ok(client_state)
    }

    /// Fetch the minimum required signatures from the attestation light client.
    pub(crate) fn attestation_client_min_sigs(
        &self,
        client_id: &str,
        light_client_program_id: Pubkey,
    ) -> Result<usize> {
        Ok(self
            .attestation_client_state(client_id, light_client_program_id)?
            .min_required_sigs as usize)
    }

    /// Fetch the attestation consensus state timestamp at a given height (seconds).
    pub(crate) fn attestation_consensus_state_timestamp_secs(
        &self,
        client_id: &str,
        height: u64,
        light_client_program_id: Pubkey,
    ) -> Result<u64> {
        use solana_ibc_types::attestation::{
            ClientState as AttestationClientState, ConsensusState as AttestationConsensusState,
        };

        let (client_state_pda, _) = AttestationClientState::pda(client_id, light_client_program_id);
        let (consensus_state_pda, _) =
            AttestationConsensusState::pda(client_state_pda, height, light_client_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&consensus_state_pda, CommitmentConfig::confirmed())
            .context("Failed to fetch attestation consensus state account")?
            .value
            .ok_or_else(|| anyhow::anyhow!("Attestation consensus state account not found"))?;

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let consensus_state = AttestationConsensusState::deserialize(&mut data)
            .context("Failed to deserialize attestation consensus state")?;

        Ok(consensus_state.timestamp)
    }

    pub(crate) fn create_tx_bytes(&self, instructions: &[Instruction]) -> Result<Vec<u8>> {
        if instructions.is_empty() {
            anyhow::bail!("No instructions to execute on Solana");
        }

        let recent_blockhash = self.get_recent_blockhash()?;

        let alt_addresses = match self.alt_address {
            Some(alt_address) => self.fetch_alt_addresses(alt_address)?,
            None => vec![],
        };

        self.create_v0_tx(instructions, recent_blockhash, alt_addresses)
    }

    pub(crate) fn create_tx_bytes_with_alt(
        &self,
        instructions: &[Instruction],
        alt_address: Pubkey,
        alt_addresses: Vec<Pubkey>,
    ) -> Result<Vec<u8>> {
        let recent_blockhash = self.get_recent_blockhash()?;

        let alt_account = AddressLookupTableAccount {
            key: alt_address,
            addresses: alt_addresses,
        };

        let v0_message =
            self.compile_v0_message_with_alt(instructions, recent_blockhash, alt_account)?;

        Self::serialize_v0_transaction(v0_message)
    }

    pub(crate) fn get_recent_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        self.target_solana_client
            .get_latest_blockhash()
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {e}"))
    }

    pub(crate) fn create_v0_tx(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
        alt_addresses: Vec<Pubkey>,
    ) -> Result<Vec<u8>> {
        let v0_message = if alt_addresses.is_empty() {
            self.compile_v0_message(instructions, recent_blockhash)?
        } else {
            let alt_account = AddressLookupTableAccount {
                key: self.alt_address.expect("ALT address should be set"),
                addresses: alt_addresses,
            };
            self.compile_v0_message_with_alt(instructions, recent_blockhash, alt_account)?
        };

        Self::serialize_v0_transaction(v0_message)
    }

    pub(crate) fn compile_v0_message(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<v0::Message> {
        v0::Message::try_compile(&self.fee_payer, instructions, &[], recent_blockhash)
            .map_err(|e| anyhow::anyhow!("Failed to compile v0 message: {e}"))
    }

    pub(crate) fn compile_v0_message_with_alt(
        &self,
        instructions: &[Instruction],
        recent_blockhash: solana_sdk::hash::Hash,
        alt_account: AddressLookupTableAccount,
    ) -> Result<v0::Message> {
        v0::Message::try_compile(
            &self.fee_payer,
            instructions,
            &[alt_account],
            recent_blockhash,
        )
        .map_err(|e| anyhow::anyhow!("Failed to compile v0 message with ALT: {e}"))
    }

    pub(crate) fn serialize_v0_transaction(v0_message: v0::Message) -> Result<Vec<u8>> {
        let num_signatures = v0_message.header.num_required_signatures as usize;
        let versioned_tx = VersionedTransaction {
            signatures: vec![solana_sdk::signature::Signature::default(); num_signatures],
            message: VersionedMessage::V0(v0_message),
        };
        let serialized_tx = bincode::serialize(&versioned_tx)?;
        Ok(serialized_tx)
    }

    pub(crate) fn fetch_alt_addresses(&self, alt_address: Pubkey) -> Result<Vec<Pubkey>> {
        let alt_account = self
            .target_solana_client
            .get_account_with_commitment(&alt_address, CommitmentConfig::confirmed())
            .map_err(|e| anyhow::anyhow!("Failed to fetch ALT account {alt_address}: {e}"))?
            .value
            .ok_or_else(|| anyhow::anyhow!("ALT account {alt_address} not found"))?;

        let lookup_table = AddressLookupTable::deserialize(&alt_account.data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize ALT: {e}"))?;

        Ok(lookup_table.addresses.to_vec())
    }

    pub(crate) fn build_create_alt_tx(&self, slot: u64) -> Result<Vec<u8>> {
        let (create_ix, _alt_address) = create_lookup_table(self.fee_payer, self.fee_payer, slot);
        self.create_tx_bytes(&[create_ix])
    }

    pub(crate) fn build_extend_alt_tx(&self, slot: u64, accounts: Vec<Pubkey>) -> Result<Vec<u8>> {
        let (alt_address, _) = derive_alt_address(slot, self.fee_payer);
        let extend_ix =
            extend_lookup_table(alt_address, self.fee_payer, Some(self.fee_payer), accounts);
        self.create_tx_bytes(&[extend_ix])
    }

    pub(crate) fn extend_compute_ix() -> Vec<Instruction> {
        let compute_budget_ix =
            ComputeBudgetInstruction::set_compute_unit_limit(MAX_COMPUTE_UNIT_LIMIT);
        let priority_fee_ix =
            ComputeBudgetInstruction::set_compute_unit_price(DEFAULT_PRIORITY_FEE);
        vec![compute_budget_ix, priority_fee_ix]
    }

    /// Create a client on Solana (stub for attestation mode).
    pub async fn create_client(
        &self,
        _parameters: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<u8>> {
        anyhow::bail!("create_client not yet implemented for eth-to-solana")
    }
}

/// Derive ALT address from current slot and authority.
#[must_use]
pub fn derive_alt_address(slot: u64, authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[authority.as_ref(), &slot.to_le_bytes()],
        &solana_sdk::address_lookup_table::program::id(),
    )
}
