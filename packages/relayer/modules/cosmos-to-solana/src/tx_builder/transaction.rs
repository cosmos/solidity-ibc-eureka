//! Transaction building utilities for Solana.

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
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
use solana_ibc_types::{
    router::{IBCApp, RouterState},
    Client,
};

use super::{DEFAULT_PRIORITY_FEE, MAX_COMPUTE_UNIT_LIMIT};

/// Helper to derive ALT address from current slot and authority
#[must_use]
pub fn derive_alt_address(slot: u64, authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[authority.as_ref(), &slot.to_le_bytes()],
        &solana_sdk::address_lookup_table::program::id(),
    )
}

impl super::TxBuilder {
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
            return Err(anyhow::anyhow!(
                "Account data too short for RouterState account"
            ));
        }

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let router_state = solana_ibc_types::RouterState::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize RouterState account: {e}"))?;

        Ok(router_state.access_manager)
    }

    /// Resolve the IBC app program ID for a given port
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
            return Err(anyhow::anyhow!("Account data too short for IBCApp account"));
        }

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;

        tracing::info!(
            "Resolved port '{}' to program ID: {}",
            port_id,
            ibc_app.app_program_id
        );

        Ok(ibc_app.app_program_id)
    }

    /// Resolve the light client program id.
    pub(crate) fn resolve_client_program_id(&self, client_id: &str) -> Result<Pubkey> {
        let (client_account, _) = Client::pda(client_id, self.solana_ics26_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&client_account, CommitmentConfig::confirmed())
            .map_err(|e| {
                anyhow::anyhow!("Failed to fetch Client account for client '{client_id}': {e}",)
            })?
            .value
            .ok_or_else(|| anyhow::anyhow!("Client account not found for client '{client_id}'"))?;

        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for Client account"));
        }

        // Deserialize Client account using borsh (skip discriminator)
        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let client = solana_ibc_types::ClientAccount::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize Client account: {e}"))?;

        tracing::info!(
            "Resolved client '{client_id}' to light client program ID: {}",
            client.client_program_id
        );

        Ok(client.client_program_id)
    }

    /// Get chain ID from Cosmos
    pub(crate) async fn chain_id(&self) -> Result<String> {
        use tendermint_rpc::Client as _;
        Ok(self
            .src_tm_client
            .latest_block()
            .await?
            .block
            .header
            .chain_id
            .into())
    }

    /// Fetch Cosmos client state from the light client on Solana.
    pub(crate) fn cosmos_client_state(
        &self,
        chain_id: &str,
        solana_ics07_program_id: Pubkey,
    ) -> Result<solana_ibc_types::ics07::ClientState> {
        use solana_ibc_types::ics07::ClientState;

        let (client_state_pda, _) = ClientState::pda(chain_id, solana_ics07_program_id);

        let account = self
            .target_solana_client
            .get_account_with_commitment(&client_state_pda, CommitmentConfig::confirmed())
            .context("Failed to fetch client state account")?
            .value
            .ok_or_else(|| anyhow::anyhow!("Client state account not found"))?;

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let client_state =
            ClientState::deserialize(&mut data).context("Failed to deserialize client state")?;

        Ok(client_state)
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

    pub(crate) fn extend_compute_ix_with_heap() -> Vec<Instruction> {
        let compute_budget_ix =
            ComputeBudgetInstruction::set_compute_unit_limit(MAX_COMPUTE_UNIT_LIMIT);
        let priority_fee_ix =
            ComputeBudgetInstruction::set_compute_unit_price(DEFAULT_PRIORITY_FEE);
        let heap_size_ix = ComputeBudgetInstruction::request_heap_frame(256 * 1024);
        vec![compute_budget_ix, priority_fee_ix, heap_size_ix]
    }
}
