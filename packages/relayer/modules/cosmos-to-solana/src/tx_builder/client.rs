//! Client operations - create/update client and signature verification.

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use ibc_client_tendermint::types::Header as TmHeader;
use solana_sdk::{
    ed25519_instruction::{Ed25519SignatureOffsets, DATA_START},
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use tendermint::{chain::Id as ChainId, vote::CanonicalVote};
use tendermint_proto::Protobuf;

use solana_ibc_types::ics07::{ics07_instructions, ClientState, ConsensusState, SignatureData};

use super::{derive_header_chunk, UploadChunkParams};

impl super::TxBuilder {
    pub(crate) fn build_create_client_instruction(
        &self,
        chain_id: &str,
        latest_height: u64,
        client_state: &ClientState,
        consensus_state: &ConsensusState,
        access_manager: Pubkey,
    ) -> Result<Instruction> {
        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (consensus_state_pda, _) = ConsensusState::pda(
            client_state_pda,
            latest_height,
            self.solana_ics07_program_id,
        );
        let (app_state_pda, _) =
            solana_ibc_types::ics07::AppState::pda(self.solana_ics07_program_id);

        let accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(consensus_state_pda, false),
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let discriminator = ics07_instructions::initialize_discriminator();

        let mut instruction_data = Vec::new();

        instruction_data.extend_from_slice(&discriminator);

        instruction_data.extend_from_slice(&chain_id.try_to_vec()?);
        instruction_data.extend_from_slice(&latest_height.try_to_vec()?);
        instruction_data.extend_from_slice(&client_state.try_to_vec()?);
        instruction_data.extend_from_slice(&consensus_state.try_to_vec()?);
        instruction_data.extend_from_slice(&access_manager.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data: instruction_data,
        })
    }

    pub(crate) fn build_assemble_and_update_client_tx(
        &self,
        chain_id: &str,
        target_height: u64,
        trusted_height: u64,
        total_chunks: u8,
        signature_data: &[SignatureData],
        alt_config: Option<(u64, Vec<Pubkey>)>,
    ) -> Result<Vec<u8>> {
        use super::transaction::derive_alt_address;
        use solana_ibc_types::AccessManager;

        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (trusted_consensus_state, _) = ConsensusState::pda(
            client_state_pda,
            trusted_height,
            self.solana_ics07_program_id,
        );
        let (new_consensus_state, _) = ConsensusState::pda(
            client_state_pda,
            target_height,
            self.solana_ics07_program_id,
        );

        let (app_state_pda, _) =
            solana_ibc_types::ics07::AppState::pda(self.solana_ics07_program_id);

        let access_manager_program_id = self.resolve_access_manager_program_id()?;
        let (access_manager, _) = AccessManager::pda(access_manager_program_id);

        let mut accounts = vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new_readonly(access_manager, false),
            AccountMeta::new_readonly(trusted_consensus_state, false),
            AccountMeta::new(new_consensus_state, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
        ];

        accounts.extend((0..total_chunks).map(|chunk_index| {
            let (chunk_pda, _) = derive_header_chunk(
                self.fee_payer,
                chain_id,
                target_height,
                chunk_index,
                self.solana_ics07_program_id,
            );
            AccountMeta::new(chunk_pda, false)
        }));

        accounts.extend(signature_data.iter().map(|sig_data| {
            let (sig_verify_pda, _) = Pubkey::find_program_address(
                &[b"sig_verify", &sig_data.signature_hash],
                &self.solana_ics07_program_id,
            );
            AccountMeta::new_readonly(sig_verify_pda, false)
        }));

        let mut data = ics07_instructions::assemble_and_update_client_discriminator().to_vec();

        let chain_id_len = u32::try_from(chain_id.len()).expect("chain_id too long");
        data.extend_from_slice(&chain_id_len.to_le_bytes());
        data.extend_from_slice(chain_id.as_bytes());
        data.extend_from_slice(&target_height.to_le_bytes());
        data.extend_from_slice(&[total_chunks]);

        let ix = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        let mut instructions = Self::extend_compute_ix_with_heap();
        instructions.push(ix);

        match alt_config {
            Some((slot, addresses)) => {
                let (alt_address, _) = derive_alt_address(slot, self.fee_payer);
                self.create_tx_bytes_with_alt(&instructions, alt_address, addresses)
            }
            None => self.create_tx_bytes(&instructions),
        }
    }

    pub(crate) fn build_cleanup_tx(
        &self,
        chain_id: &str,
        target_height: u64,
        total_chunks: u8,
        signature_data: &[SignatureData],
    ) -> Result<Vec<u8>> {
        let mut accounts = vec![AccountMeta::new(self.fee_payer, true)];

        accounts.extend((0..total_chunks).map(|chunk_index| {
            let (chunk_pda, _) = derive_header_chunk(
                self.fee_payer,
                chain_id,
                target_height,
                chunk_index,
                self.solana_ics07_program_id,
            );
            AccountMeta::new(chunk_pda, false)
        }));

        accounts.extend(signature_data.iter().map(|sig_data| {
            let (sig_verify_pda, _) = Pubkey::find_program_address(
                &[b"sig_verify", &sig_data.signature_hash],
                &self.solana_ics07_program_id,
            );
            AccountMeta::new(sig_verify_pda, false)
        }));

        let mut data = ics07_instructions::cleanup_incomplete_upload_discriminator().to_vec();
        data.extend_from_slice(&self.fee_payer.try_to_vec()?);

        let instruction = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        let mut instructions = Self::extend_compute_ix();
        instructions.push(instruction);

        self.create_tx_bytes(&instructions)
    }

    pub(crate) fn build_upload_header_chunk_instruction(
        &self,
        chain_id: &str,
        target_height: u64,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> Result<Instruction> {
        let params = UploadChunkParams {
            chain_id: chain_id.to_string(),
            target_height,
            chunk_index,
            chunk_data,
        };

        let (client_state_pda, _) = ClientState::pda(chain_id, self.solana_ics07_program_id);
        let (chunk_pda, _) = derive_header_chunk(
            self.fee_payer,
            chain_id,
            target_height,
            chunk_index,
            self.solana_ics07_program_id,
        );

        let accounts = vec![
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new_readonly(client_state_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let mut data = ics07_instructions::upload_header_chunk_discriminator().to_vec();
        data.extend_from_slice(&params.try_to_vec()?);

        Ok(Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        })
    }

    pub(crate) fn build_chunk_transactions(
        &self,
        chunks: &[Vec<u8>],
        chain_id: &str,
        target_height: u64,
    ) -> Result<Vec<Vec<u8>>> {
        chunks
            .iter()
            .enumerate()
            .map(|(index, chunk_data)| {
                let chunk_index = u8::try_from(index)
                    .map_err(|_| anyhow::anyhow!("Chunk index {index} exceeds u8 max"))?;
                let upload_ix = self.build_upload_header_chunk_instruction(
                    chain_id,
                    target_height,
                    chunk_index,
                    chunk_data.clone(),
                )?;
                self.create_tx_bytes(&[upload_ix])
            })
            .collect()
    }

    /// Extracts signature data from Protobuf Tendermint Header for Ed25519 pre-verification
    pub(crate) fn extract_signature_data_from_header(
        header: &ibc_client_tendermint::types::Header,
        chain_id: &str,
    ) -> Result<Vec<SignatureData>> {
        use tendermint::validator::Info as ValidatorInfo;

        let commit = &header.signed_header.commit;
        let validators = &header.validator_set.validators();

        let chain_id =
            ChainId::try_from(chain_id.to_string()).context("Failed to parse chain ID")?;

        let mut signature_data_vec = Vec::new();
        let mut seen_hashes = std::collections::HashSet::new();
        let mut duplicates_skipped = 0;

        for (idx, commit_sig) in commit.signatures.iter().enumerate() {
            let (validator_address, timestamp, signature_opt) = match commit_sig {
                tendermint::block::CommitSig::BlockIdFlagCommit {
                    validator_address,
                    timestamp,
                    signature,
                }
                | tendermint::block::CommitSig::BlockIdFlagNil {
                    validator_address,
                    timestamp,
                    signature,
                } => (validator_address, timestamp, signature),
                tendermint::block::CommitSig::BlockIdFlagAbsent => continue,
            };

            let Some(signature_bytes) = signature_opt else {
                continue;
            };

            let validator: &ValidatorInfo = validators
                .iter()
                .find(|v| v.address == *validator_address)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Validator address {validator_address:?} not found in validator set",
                    )
                })?;

            let pubkey = match &validator.pub_key {
                tendermint::PublicKey::Ed25519(key) => key.as_bytes(),
                _ => {
                    anyhow::bail!("Only Ed25519 keys are supported for signature verification");
                }
            };

            let canonical_vote = CanonicalVote {
                vote_type: tendermint::vote::Type::Precommit,
                height: commit.height,
                round: commit.round,
                block_id: Some(commit.block_id),
                timestamp: Some(*timestamp),
                chain_id: chain_id.clone(),
            };

            let sign_bytes = <CanonicalVote as Protobuf<
                tendermint_proto::v0_38::types::CanonicalVote,
            >>::encode_length_delimited_vec(canonical_vote);

            let signature: [u8; 64] = signature_bytes
                .as_bytes()
                .try_into()
                .context("Signature must be 64 bytes")?;

            let signature_hash =
                solana_sdk::hash::hashv(&[pubkey, sign_bytes.as_slice(), &signature]).to_bytes();

            if !seen_hashes.insert(signature_hash) {
                duplicates_skipped += 1;
                tracing::info!(
                    "Skipping duplicate signature at index {} with hash {:?}",
                    idx,
                    &signature_hash[..8]
                );
                continue;
            }

            signature_data_vec.push(SignatureData {
                signature_hash,
                pubkey: pubkey.try_into().context("Public key must be 32 bytes")?,
                msg: sign_bytes,
                signature,
            });
        }

        tracing::info!(
            "Extracted {} signatures for pre-verification (out of {} total, {} duplicates skipped)",
            signature_data_vec.len(),
            commit.signatures.len(),
            duplicates_skipped
        );

        Ok(signature_data_vec)
    }

    /// Select minimal signatures to meet 2/3 threshold
    pub(crate) fn select_minimal_signatures(
        signature_data: &[SignatureData],
        header: &TmHeader,
        trust_numerator: u64,
        trust_denominator: u64,
    ) -> Result<Vec<SignatureData>> {
        let untrusted_validator_set = &header.validator_set;
        let untrusted_total_power: u64 = untrusted_validator_set.total_voting_power().into();
        let untrusted_required_power = (untrusted_total_power * 2) / 3;

        let mut accumulated_power = 0u64;
        let mut selected = Vec::new();

        for (val_idx, validator) in untrusted_validator_set.validators().iter().enumerate() {
            let pubkey_bytes = validator.pub_key.to_bytes();

            if let Some(sig_data) = signature_data.iter().find(|sig| pubkey_bytes == sig.pubkey) {
                accumulated_power += validator.power();
                selected.push(sig_data.clone());

                if accumulated_power >= untrusted_required_power {
                    tracing::info!(
                        "Selected {} signatures reaching 2/3 at validator {}/{}",
                        selected.len(),
                        val_idx + 1,
                        untrusted_validator_set.validators().len()
                    );
                    break;
                }
            }
        }

        if accumulated_power < untrusted_required_power {
            anyhow::bail!(
                "Insufficient voting power: {accumulated_power} < {untrusted_required_power} required",
            );
        }

        let trusted_validator_set = &header.trusted_next_validator_set;
        let trusted_total_power: u64 = trusted_validator_set.total_voting_power().into();
        let trusted_required_power = (trusted_total_power * trust_numerator) / trust_denominator;

        let mut trusted_power = 0u64;
        let selected_pubkeys: std::collections::HashSet<_> =
            selected.iter().map(|s| s.pubkey.as_slice()).collect();

        for validator in trusted_validator_set.validators() {
            if selected_pubkeys.contains(validator.pub_key.to_bytes().as_slice()) {
                trusted_power += validator.power();
            }
        }

        if trusted_power < trusted_required_power {
            anyhow::bail!(
                "Selection fails trusted threshold: {trusted_power} < {trusted_required_power} required",
            );
        }

        Ok(selected)
    }

    /// Builds a single signature pre-verification transaction
    #[allow(deprecated)]
    pub(crate) fn build_pre_verify_signature_transaction(
        &self,
        sig_data: &SignatureData,
    ) -> Result<Vec<u8>> {
        let mut instruction_data = vec![
            1u8, // number of signatures
            0u8, // padding
        ];

        let data_start = u16::try_from(DATA_START).expect("DATA_START (16) must fit in u16");
        let pubkey_offset = data_start;
        let signature_offset = data_start + 32;
        let message_data_offset = data_start + 32 + 64;
        let message_data_size = u16::try_from(sig_data.msg.len())
            .context("CanonicalVote message exceeds 65,535 bytes (Ed25519 instruction limit)")?;

        let offsets = Ed25519SignatureOffsets {
            signature_offset,
            signature_instruction_index: u16::MAX,
            public_key_offset: pubkey_offset,
            public_key_instruction_index: u16::MAX,
            message_data_offset,
            message_data_size,
            message_instruction_index: u16::MAX,
        };

        #[allow(clippy::borrow_as_ptr)]
        let offsets_bytes =
            unsafe { std::slice::from_raw_parts((&raw const offsets).cast::<u8>(), 14) };
        instruction_data.extend_from_slice(offsets_bytes);

        instruction_data.extend_from_slice(&sig_data.pubkey);
        instruction_data.extend_from_slice(&sig_data.signature);
        instruction_data.extend_from_slice(&sig_data.msg);

        let ed25519_ix = Instruction {
            program_id: solana_sdk::ed25519_program::ID,
            accounts: vec![],
            data: instruction_data,
        };

        let (sig_verify_pda, _) = Pubkey::find_program_address(
            &[b"sig_verify", &sig_data.signature_hash],
            &self.solana_ics07_program_id,
        );

        let accounts = vec![
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            AccountMeta::new(sig_verify_pda, false),
            AccountMeta::new(self.fee_payer, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        ];

        let params_data = sig_data.try_to_vec()?;

        let mut data = ics07_instructions::pre_verify_signature_discriminator().to_vec();
        data.extend_from_slice(&params_data);

        let pre_verify_ix = Instruction {
            program_id: self.solana_ics07_program_id,
            accounts,
            data,
        };

        let tx_bytes = self.create_tx_bytes(&[ed25519_ix, pre_verify_ix])?;

        Ok(tx_bytes)
    }
}
