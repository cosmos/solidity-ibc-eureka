//! ABI payload account extraction for Eth→Solana relay.
//!
//! When relaying from Ethereum to Solana, packet payloads arrive with
//! `encoding = "application/x-solidity-abi"` and ABI-encoded values.
//! The GmpSolanaPayload (accounts + instruction data + payer position) is
//! ABI-encoded directly in the IBC packet payload, committed on Ethereum.
//! The relayer decodes this to build the remaining accounts for recv_packet.

use alloy::sol_types::SolValue;
use anyhow::Result;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use crate::gmp::GMP_PORT_ID;

use super::SolanaTxBuilder;

/// ABI encoding identifier used by Ethereum ICS27 GMP.
pub(crate) const ABI_ENCODING: &str = "application/x-solidity-abi";

/// Packed account entry size: pubkey(32) + is_signer(1) + is_writable(1)
const PACKED_ACCOUNT_SIZE: usize = 34;

// ABI type matching Solidity's `IICS27GMPMsgs.GMPPacketData`.
alloy::sol! {
    struct AbiGmpPacketData {
        string sender;
        string receiver;
        bytes salt;
        bytes payload;
        string memo;
    }
}

// ABI type matching the inner GmpSolanaPayload encoding:
// `abi.encode(bytes packedAccounts, bytes instructionData, uint32 payerPosition)`
alloy::sol! {
    struct AbiGmpSolanaPayload {
        bytes packedAccounts;
        bytes instructionData;
        uint32 payerPosition;
    }
}

/// Extracted GMP account info from ABI-encoded payloads.
pub struct AbiGmpAccountInfo {
    /// GMP remaining accounts for the recv_packet instruction.
    /// Layout: [gmp_pda, target_program, execution_accounts...]
    pub gmp_accounts: Vec<AccountMeta>,
}

impl SolanaTxBuilder {
    /// Extract GMP accounts for ABI-encoded GMP payloads, if applicable.
    ///
    /// Returns `None` if the payload is not ABI-encoded or not a GMP packet.
    pub fn extract_abi_gmp_accounts(
        &self,
        msg: &ibc_proto_eureka::ibc::core::channel::v2::MsgRecvPacket,
    ) -> Result<Option<AbiGmpAccountInfo>> {
        let packet = match msg.packet.as_ref() {
            Some(p) => p,
            None => return Ok(None),
        };

        let dest_client = &packet.destination_client;

        for payload in &packet.payloads {
            if payload.encoding != ABI_ENCODING {
                continue;
            }

            if payload.destination_port != GMP_PORT_ID {
                continue;
            }

            return self
                .extract_single_abi_gmp_accounts(payload, dest_client)
                .map(Some);
        }

        Ok(None)
    }

    /// Extract GMP accounts from a single ABI-encoded GMP payload.
    fn extract_single_abi_gmp_accounts(
        &self,
        payload: &ibc_proto_eureka::ibc::core::channel::v2::Payload,
        dest_client: &str,
    ) -> Result<AbiGmpAccountInfo> {
        tracing::info!("Extracting ABI GMP accounts for dest_client={dest_client}");

        // 1. ABI-decode the outer GMPPacketData
        let abi_gmp: AbiGmpPacketData = SolValue::abi_decode(&payload.value)
            .map_err(|e| anyhow::anyhow!("Failed to ABI decode GMPPacketData: {e}"))?;

        tracing::debug!(
            sender = %abi_gmp.sender,
            receiver = %abi_gmp.receiver,
            payload_len = abi_gmp.payload.len(),
            "Decoded ABI GMPPacketData"
        );

        // 2. Parse receiver as the target program ID
        let target_program: Pubkey = abi_gmp
            .receiver
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid receiver as Solana pubkey: {e}"))?;

        // 3. ABI-decode the inner GmpSolanaPayload from the packet payload.
        //    Use abi_decode_params (not abi_decode) because constructMintCall returns
        //    abi.encode(bytes, bytes, uint32) — three separate params without an outer
        //    tuple offset, unlike abi.encode(structVar) which wraps in a single-element tuple.
        let abi_solana: AbiGmpSolanaPayload = SolValue::abi_decode_params(&abi_gmp.payload)
            .map_err(|e| anyhow::anyhow!("Failed to ABI decode GmpSolanaPayload: {e}"))?;

        tracing::debug!(
            packed_accounts_len = abi_solana.packedAccounts.len(),
            instruction_data_len = abi_solana.instructionData.len(),
            payer_position = abi_solana.payerPosition,
            "Decoded ABI GmpSolanaPayload"
        );

        // 4. Parse packed accounts
        let packed = &abi_solana.packedAccounts;
        if packed.len() % PACKED_ACCOUNT_SIZE != 0 {
            anyhow::bail!(
                "Packed accounts length {} is not a multiple of {}",
                packed.len(),
                PACKED_ACCOUNT_SIZE
            );
        }

        // 5. Build GMP account PDA from ABI-decoded fields
        let sender: solana_ibc_proto::Sender = abi_gmp
            .sender
            .clone()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid sender"))?;
        let salt: solana_ibc_proto::Salt = abi_gmp
            .salt
            .to_vec()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid salt"))?;

        let gmp_program_id = self
            .resolve_port_program_id(GMP_PORT_ID)
            .map_err(|e| anyhow::anyhow!("Failed to resolve GMP program ID: {e}"))?;

        let client_id = solana_ibc_types::ClientId::new(dest_client)
            .map_err(|e| anyhow::anyhow!("Invalid client ID: {e:?}"))?;

        let gmp_account =
            solana_ibc_types::GMPAccount::new(client_id, sender, salt, &gmp_program_id);
        let (gmp_account_pda, _) = gmp_account.pda();

        // 6. Build remaining accounts: [gmp_pda, target_program, execution_accounts...]
        let mut gmp_accounts = vec![
            AccountMeta {
                pubkey: gmp_account_pda,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: target_program,
                is_signer: false,
                is_writable: false,
            },
        ];

        // Add execution accounts from packed bytes
        for chunk in packed.chunks_exact(PACKED_ACCOUNT_SIZE) {
            let pubkey_bytes: [u8; 32] = chunk[..32]
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid pubkey in packed accounts"))?;
            let is_writable = chunk[33] != 0;

            gmp_accounts.push(AccountMeta {
                pubkey: Pubkey::from(pubkey_bytes),
                is_signer: false,
                is_writable,
            });
        }

        tracing::info!(
            gmp_pda = %gmp_account_pda,
            num_accounts = gmp_accounts.len(),
            "Extracted ABI GMP accounts"
        );

        Ok(AbiGmpAccountInfo { gmp_accounts })
    }
}
