//! Packet instruction builders for recv, ack, and timeout packets.

use anchor_lang::prelude::*;
use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::gmp;
use solana_ibc_types::ics07::{ClientState, ConsensusState};
use solana_ibc_types::{
    router::{router_instructions, Client, Commitment, IBCApp, IBCAppState, RouterState},
    AccessManager, MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket,
};

use super::TimeoutAccountsParams;
use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;

impl super::TxBuilder {
    pub(crate) fn build_recv_packet_instruction(
        &self,
        chain_id: &str,
        msg: &MsgRecvPacket,
        chunk_accounts: Vec<Pubkey>,
        payload_data: &[Vec<u8>],
    ) -> Result<Instruction> {
        let dest_port = if msg.packet.payloads.is_empty() {
            let [metadata] = msg.payloads.as_slice() else {
                return Err(anyhow::anyhow!(
                    "Expected exactly one recv packet payload metadata element"
                ));
            };
            &metadata.dest_port
        } else {
            let [payload] = msg.packet.payloads.as_slice() else {
                return Err(anyhow::anyhow!(
                    "Expected exactly one recv packet payload element"
                ));
            };
            &payload.dest_port
        };

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app, _) = IBCApp::pda(dest_port, self.solana_ics26_program_id);

        let (packet_receipt, _) = Commitment::packet_receipt_pda(
            &msg.packet.dest_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (packet_ack, _) = Commitment::packet_ack_pda(
            &msg.packet.dest_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (client, _) = Client::pda(&msg.packet.dest_client, self.solana_ics26_program_id);

        let (client_state, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);

        let (consensus_state, _) =
            ConsensusState::pda(client_state, msg.proof.height, self.solana_ics07_program_id);

        let ibc_app_program_id = self.resolve_port_program_id(dest_port)?;

        let (ibc_app_state, _) = IBCAppState::pda(dest_port, ibc_app_program_id);

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new_readonly(ibc_app, false),
            AccountMeta::new(packet_receipt, false),
            AccountMeta::new(packet_ack, false),
            AccountMeta::new_readonly(ibc_app_program_id, false),
            AccountMeta::new(ibc_app_state, false),
            AccountMeta::new_readonly(self.solana_ics26_program_id, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        for chunk_account in chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

        let (dest_port_for_gmp, encoding, payload_value) = if msg.packet.payloads.is_empty() {
            let metadata = &msg.payloads[0];
            let data = &payload_data[0];
            (
                metadata.dest_port.as_str(),
                metadata.encoding.as_str(),
                data.as_slice(),
            )
        } else {
            let payload = &msg.packet.payloads[0];
            (
                payload.dest_port.as_str(),
                payload.encoding.as_str(),
                payload.value.as_slice(),
            )
        };

        let gmp_accounts = gmp::extract_gmp_accounts(
            dest_port_for_gmp,
            encoding,
            payload_value,
            &msg.packet.dest_client,
            ibc_app_program_id,
        )?;
        accounts.extend(gmp_accounts);

        let mut data = router_instructions::recv_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    pub(crate) async fn build_ack_packet_instruction(
        &self,
        msg: &MsgAckPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        use solana_sdk::commitment_config::CommitmentConfig;

        tracing::info!(
            "Building ack packet instruction for packet from {} to {}, sequence {}",
            msg.packet.source_client,
            msg.packet.dest_client,
            msg.packet.sequence
        );

        let solana_ics26_program_id = self.solana_ics26_program_id;

        let (router_state, _) = RouterState::pda(solana_ics26_program_id);

        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one ack packet payload element"
            ));
        };

        let source_port = payload.source_port.clone();

        let (ibc_app_pda, _) = IBCApp::pda(&source_port, solana_ics26_program_id);

        let ibc_app_account = self
            .target_solana_client
            .get_account_with_commitment(&ibc_app_pda, CommitmentConfig::confirmed())
            .map_err(|e| anyhow::anyhow!("Failed to get IBC app account: {e}"))?
            .value
            .ok_or_else(|| anyhow::anyhow!("IBC app account not found"))?;

        if ibc_app_account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            return Err(anyhow::anyhow!("Account data too short for IBCApp account"));
        }

        let mut data = &ibc_app_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;

        let ibc_app_program = ibc_app.app_program_id;
        tracing::info!("IBC app program ID: {}", ibc_app_program);

        let (app_state, _) = IBCAppState::pda(&source_port, ibc_app_program);

        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            solana_ics26_program_id,
        );

        let (client, _) = Client::pda(&msg.packet.source_client, solana_ics26_program_id);
        tracing::info!(
            "Router client PDA for '{}': {}",
            msg.packet.source_client,
            client
        );

        let chain_id = self.chain_id().await?;
        tracing::info!("Cosmos chain ID for ICS07 derivation: {}", chain_id);

        let (client_state, _) = ClientState::pda(&chain_id, self.solana_ics07_program_id);
        tracing::info!("ICS07 client state PDA: {}", client_state);

        let (consensus_state, _) =
            ConsensusState::pda(client_state, msg.proof.height, self.solana_ics07_program_id);

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = solana_ibc_types::AccessManager::pda(access_manager_program_id);

        let mut accounts = vec![
            AccountMeta::new_readonly(router_state, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new(packet_commitment, false),
            AccountMeta::new_readonly(ibc_app_program, false),
            AccountMeta::new(app_state, false),
            AccountMeta::new_readonly(self.solana_ics26_program_id, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(client, false),
            AccountMeta::new_readonly(self.solana_ics07_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];

        for chunk_account in chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

        let mut data = router_instructions::ack_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    pub(crate) fn build_timeout_packet_instruction(
        &self,
        chain_id: &str,
        msg: &MsgTimeoutPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        tracing::info!(
            "Building timeout packet instruction for packet from {} to {}, sequence {}",
            msg.packet.source_client,
            msg.packet.dest_client,
            msg.packet.sequence
        );

        let source_port = Self::extract_timeout_source_port(msg)?;
        let accounts = self.build_timeout_accounts_with_derived_keys(
            chain_id,
            msg,
            &source_port,
            chunk_accounts,
        )?;
        let data = Self::build_timeout_instruction_data(msg)?;

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    pub(crate) fn extract_timeout_source_port(msg: &MsgTimeoutPacket) -> Result<String> {
        let [payload] = msg.packet.payloads.as_slice() else {
            return Err(anyhow::anyhow!(
                "Expected exactly one timeout packet payload element"
            ));
        };
        Ok(payload.source_port.clone())
    }

    #[allow(clippy::cognitive_complexity)]
    pub(crate) fn build_timeout_accounts_with_derived_keys(
        &self,
        chain_id: &str,
        msg: &MsgTimeoutPacket,
        source_port: &str,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Vec<AccountMeta>> {
        let program_id = self.solana_ics26_program_id;

        let (router_state, _) = RouterState::pda(program_id);
        let (ibc_app, _) = IBCApp::pda(source_port, program_id);
        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            program_id,
        );

        let ibc_app_program_id = self.resolve_port_program_id(source_port)?;
        let (ibc_app_state, _) = IBCAppState::pda(source_port, ibc_app_program_id);
        let (client, _) = Client::pda(&msg.packet.source_client, program_id);
        let (client_state, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (consensus_state, _) =
            ConsensusState::pda(client_state, msg.proof.height, self.solana_ics07_program_id);

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = solana_ibc_types::AccessManager::pda(access_manager_program_id);

        Ok(Self::assemble_timeout_accounts(TimeoutAccountsParams {
            access_manager,
            router_state,
            ibc_app,
            packet_commitment,
            ibc_app_program_id,
            ibc_app_state,
            client,
            client_state,
            consensus_state,
            fee_payer: self.fee_payer,
            router_program_id: self.solana_ics26_program_id,
            light_client_program_id: self.solana_ics07_program_id,
            chunk_accounts,
        }))
    }

    pub(crate) fn assemble_timeout_accounts(params: TimeoutAccountsParams) -> Vec<AccountMeta> {
        let mut accounts = vec![
            AccountMeta::new_readonly(params.router_state, false),
            AccountMeta::new_readonly(params.access_manager, false),
            AccountMeta::new_readonly(params.ibc_app, false),
            AccountMeta::new(params.packet_commitment, false),
            AccountMeta::new_readonly(params.ibc_app_program_id, false),
            AccountMeta::new(params.ibc_app_state, false),
            AccountMeta::new_readonly(params.router_program_id, false),
            AccountMeta::new(params.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new_readonly(params.client, false),
            AccountMeta::new_readonly(params.light_client_program_id, false),
            AccountMeta::new_readonly(params.client_state, false),
            AccountMeta::new_readonly(params.consensus_state, false),
        ];

        for chunk_account in params.chunk_accounts {
            accounts.push(AccountMeta::new(chunk_account, false));
        }

        accounts
    }

    pub(crate) fn build_timeout_instruction_data(msg: &MsgTimeoutPacket) -> Result<Vec<u8>> {
        let mut data = router_instructions::timeout_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);
        Ok(data)
    }
}
