//! Packet instruction builders for recv, ack, and timeout packets.

use anchor_lang::prelude::*;
use anyhow::Result;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::gmp;
use solana_ibc_types::attestation::{
    ClientState as AttestationClientState, ConsensusState as AttestationConsensusState,
};
use solana_ibc_types::ics07::{ClientState, ConsensusState};
use solana_ibc_types::{
    router::{router_instructions, Client, Commitment, IBCApp, IBCAppState, RouterState},
    AccessManager, MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket,
};

use super::TimeoutAccountsParams;
use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;

/// Derives client state and consensus state PDAs based on client type.
fn derive_light_client_pdas(
    client_id: &str,
    chain_id: &str,
    height: u64,
    light_client_program_id: Pubkey,
) -> (Pubkey, Pubkey) {
    match solana_ibc_constants::client_type_from_id(client_id) {
        Some(solana_ibc_constants::CLIENT_TYPE_ATTESTATION) => {
            let (cs, _) = AttestationClientState::pda(client_id, light_client_program_id);
            let (cons, _) = AttestationConsensusState::pda(cs, height, light_client_program_id);
            (cs, cons)
        }
        Some(solana_ibc_constants::CLIENT_TYPE_TENDERMINT) | _ => {
            let (cs, _) = ClientState::pda(chain_id, light_client_program_id);
            let (cons, _) = ConsensusState::pda(cs, height, light_client_program_id);
            (cs, cons)
        }
    }
}

/// Extracted payload info for recv packet processing.
struct RecvPayloadInfo<'a> {
    dest_port: &'a str,
    encoding: &'a str,
    value: &'a [u8],
}

/// Extract `source_port` from either inline payloads or chunked metadata.
fn extract_source_port<'a>(
    packet_payloads: &'a [solana_ibc_types::Payload],
    metadata_payloads: &'a [solana_ibc_types::router::PayloadMetadata],
    context: &str,
) -> Result<&'a str> {
    if !packet_payloads.is_empty() {
        let [payload] = packet_payloads else {
            anyhow::bail!(
                "Expected exactly one {context} packet payload element, got {}",
                packet_payloads.len()
            );
        };
        Ok(&payload.source_port)
    } else if !metadata_payloads.is_empty() {
        let [payload_meta] = metadata_payloads else {
            anyhow::bail!(
                "Expected exactly one {context} packet payload metadata element, got {}",
                metadata_payloads.len()
            );
        };
        Ok(&payload_meta.source_port)
    } else {
        anyhow::bail!("No payload data found in either packet.payloads or payloads metadata");
    }
}

/// Extract payload info from either `packet.payloads` or metadata + `payload_data`.
fn extract_recv_payload_info<'a>(
    msg: &'a MsgRecvPacket,
    payload_data: &'a [Vec<u8>],
) -> Result<RecvPayloadInfo<'a>> {
    if msg.packet.payloads.is_empty() {
        let [metadata] = msg.payloads.as_slice() else {
            anyhow::bail!("Expected exactly one recv packet payload metadata element");
        };
        let value = payload_data
            .first()
            .ok_or_else(|| anyhow::anyhow!("Missing payload data"))?
            .as_slice();
        Ok(RecvPayloadInfo {
            dest_port: &metadata.dest_port,
            encoding: &metadata.encoding,
            value,
        })
    } else {
        let [payload] = msg.packet.payloads.as_slice() else {
            anyhow::bail!("Expected exactly one recv packet payload element");
        };
        Ok(RecvPayloadInfo {
            dest_port: &payload.dest_port,
            encoding: &payload.encoding,
            value: &payload.value,
        })
    }
}

impl super::TxBuilder {
    pub(crate) fn build_recv_packet_instruction(
        &self,
        chain_id: &str,
        msg: &MsgRecvPacket,
        chunk_accounts: Vec<Pubkey>,
        payload_data: &[Vec<u8>],
    ) -> Result<Instruction> {
        let payload_info = extract_recv_payload_info(msg, payload_data)?;

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app, _) = IBCApp::pda(payload_info.dest_port, self.solana_ics26_program_id);
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

        // Resolve the light client program ID for this client
        let light_client_program_id = self.resolve_client_program_id(&msg.packet.dest_client)?;
        let (client_state, consensus_state) = derive_light_client_pdas(
            &msg.packet.dest_client,
            chain_id,
            msg.proof.height,
            light_client_program_id,
        );

        let ibc_app_program_id = self.resolve_port_program_id(payload_info.dest_port)?;
        let (ibc_app_state, _) = IBCAppState::pda(payload_info.dest_port, ibc_app_program_id);
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
            AccountMeta::new_readonly(light_client_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];
        accounts.extend(
            chunk_accounts
                .into_iter()
                .map(|a| AccountMeta::new(a, false)),
        );

        let gmp_accounts = gmp::extract_gmp_accounts(
            payload_info.dest_port,
            payload_info.encoding,
            payload_info.value,
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

    pub(crate) async fn build_ack_packet_instruction(
        &self,
        msg: &MsgAckPacket,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Instruction> {
        let source_port = extract_source_port(&msg.packet.payloads, &msg.payloads, "ack")?;

        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app_pda, _) = IBCApp::pda(source_port, self.solana_ics26_program_id);

        let ibc_app_account = self
            .target_solana_client
            .get_account_with_commitment(&ibc_app_pda, CommitmentConfig::confirmed())
            .map_err(|e| anyhow::anyhow!("Failed to get IBC app account: {e}"))?
            .value
            .ok_or_else(|| anyhow::anyhow!("IBC app account not found"))?;

        if ibc_app_account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            anyhow::bail!("Account data too short for IBCApp account");
        }

        let mut account_data = &ibc_app_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let ibc_app = solana_ibc_types::IBCApp::deserialize(&mut account_data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize IBCApp account: {e}"))?;
        let ibc_app_program = ibc_app.app_program_id;

        let (app_state, _) = IBCAppState::pda(source_port, ibc_app_program);
        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );
        let (client, _) = Client::pda(&msg.packet.source_client, self.solana_ics26_program_id);

        // Resolve the light client program ID for this client
        let light_client_program_id = self.resolve_client_program_id(&msg.packet.source_client)?;
        let chain_id = self.chain_id().await?;
        let (client_state, consensus_state) = derive_light_client_pdas(
            &msg.packet.source_client,
            &chain_id,
            msg.proof.height,
            light_client_program_id,
        );

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

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
            AccountMeta::new_readonly(light_client_program_id, false),
            AccountMeta::new_readonly(client_state, false),
            AccountMeta::new_readonly(consensus_state, false),
        ];
        accounts.extend(
            chunk_accounts
                .into_iter()
                .map(|a| AccountMeta::new(a, false)),
        );

        // Add GMP result PDA for GMP packets (will be initialized by on_ack_packet)
        // Note: IFT finalize_transfer is handled as a separate transaction after ack completes
        if let Some(result_pda) = gmp::find_gmp_result_pda(
            source_port,
            &msg.packet.source_client,
            msg.packet.sequence,
            ibc_app_program,
        ) {
            accounts.push(AccountMeta::new(result_pda, false));
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
        let source_port = extract_source_port(&msg.packet.payloads, &msg.payloads, "timeout")?;

        let mut accounts = self.build_timeout_accounts_with_derived_keys(
            chain_id,
            msg,
            source_port,
            chunk_accounts,
        )?;

        // Add GMP result PDA for GMP packets (will be initialized by on_timeout_packet)
        // Note: IFT finalize_transfer is handled as a separate transaction after timeout completes
        let ibc_app_program_id = self.resolve_port_program_id(source_port)?;
        if let Some(result_pda) = gmp::find_gmp_result_pda(
            source_port,
            &msg.packet.source_client,
            msg.packet.sequence,
            ibc_app_program_id,
        ) {
            accounts.push(AccountMeta::new(result_pda, false));
        }

        let data = Self::build_timeout_instruction_data(msg)?;

        Ok(Instruction {
            program_id: self.solana_ics26_program_id,
            accounts,
            data,
        })
    }

    pub(crate) fn build_timeout_accounts_with_derived_keys(
        &self,
        chain_id: &str,
        msg: &MsgTimeoutPacket,
        source_port: &str,
        chunk_accounts: Vec<Pubkey>,
    ) -> Result<Vec<AccountMeta>> {
        let (router_state, _) = RouterState::pda(self.solana_ics26_program_id);
        let (ibc_app, _) = IBCApp::pda(source_port, self.solana_ics26_program_id);
        let (packet_commitment, _) = Commitment::packet_commitment_pda(
            &msg.packet.source_client,
            msg.packet.sequence,
            self.solana_ics26_program_id,
        );

        let ibc_app_program_id = self.resolve_port_program_id(source_port)?;
        let (ibc_app_state, _) = IBCAppState::pda(source_port, ibc_app_program_id);
        let (client, _) = Client::pda(&msg.packet.source_client, self.solana_ics26_program_id);

        // Resolve the light client program ID for this client
        let light_client_program_id = self.resolve_client_program_id(&msg.packet.source_client)?;
        let (client_state, consensus_state) = derive_light_client_pdas(
            &msg.packet.source_client,
            chain_id,
            msg.proof.height,
            light_client_program_id,
        );

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

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
            light_client_program_id,
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
        accounts.extend(
            params
                .chunk_accounts
                .into_iter()
                .map(|a| AccountMeta::new(a, false)),
        );
        accounts
    }

    pub(crate) fn build_timeout_instruction_data(msg: &MsgTimeoutPacket) -> Result<Vec<u8>> {
        let mut data = router_instructions::timeout_packet_discriminator().to_vec();
        data.extend_from_slice(&msg.try_to_vec()?);
        Ok(data)
    }
}
