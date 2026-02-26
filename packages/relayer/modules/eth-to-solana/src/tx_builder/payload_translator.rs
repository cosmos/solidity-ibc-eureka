//! ABI payload hint builder for Eth→Solana relay.
//!
//! When relaying from Ethereum to Solana, packet payloads arrive with
//! `encoding = "application/x-solidity-abi"` and ABI-encoded values.
//! Instead of translating the payload (which breaks IBC commitment verification),
//! we build a `SolanaPayloadHint` that the GMP program reads to execute the CPI.
//! The original ABI payload is kept intact for commitment verification.

use std::sync::{Arc, LazyLock};

use alloy::sol_types::SolValue;
use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::gmp::GMP_PORT_ID;
use crate::ift_payload;
use crate::proto::Protobuf;

use super::SolanaTxBuilder;

/// ABI encoding identifier used by Ethereum ICS27 GMP.
pub(crate) const ABI_ENCODING: &str = "application/x-solidity-abi";

/// Payload hint PDA seed (must match GMP program's `SolanaPayloadHint::SEED`).
const PAYLOAD_HINT_SEED: &[u8] = b"payload_hint";

/// Anchor discriminator for `IFTBridge` accounts.
static IFT_BRIDGE_DISCRIMINATOR: LazyLock<[u8; 8]> = LazyLock::new(|| {
    let mut hasher = Sha256::new();
    hasher.update(b"account:IFTBridge");
    let result = hasher.finalize();
    result[..8].try_into().expect("sha256 produces 32 bytes")
});

/// Anchor discriminator for `store_payload_hint` instruction.
static STORE_PAYLOAD_HINT_DISCRIMINATOR: LazyLock<[u8; 8]> = LazyLock::new(|| {
    let mut hasher = Sha256::new();
    hasher.update(b"global:store_payload_hint");
    let result = hasher.finalize();
    result[..8].try_into().expect("sha256 produces 32 bytes")
});

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

/// Info needed to handle an ABI-encoded GMP payload.
pub struct AbiHintInfo {
    /// Store hint instruction (to be sent before recv_packet).
    pub store_hint_instruction: Instruction,
    /// GMP remaining accounts for the recv_packet instruction.
    /// Layout: [gmp_pda, target_program, hint_pda, execution_accounts...]
    pub gmp_accounts: Vec<AccountMeta>,
}

impl SolanaTxBuilder {
    /// Build hint info for ABI-encoded GMP payloads, if applicable.
    ///
    /// Returns `None` if the payload is not ABI-encoded or not a GMP packet.
    /// The original payload data is NOT modified.
    pub fn build_abi_hint_if_needed(
        &self,
        msg: &ibc_proto_eureka::ibc::core::channel::v2::MsgRecvPacket,
    ) -> Result<Option<AbiHintInfo>> {
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

            return self.build_single_abi_hint(payload, dest_client).map(Some);
        }

        Ok(None)
    }

    /// Build hint info for a single ABI-encoded GMP payload.
    fn build_single_abi_hint(
        &self,
        payload: &ibc_proto_eureka::ibc::core::channel::v2::Payload,
        dest_client: &str,
    ) -> Result<AbiHintInfo> {
        tracing::info!("Building ABI hint for dest_client={dest_client}");

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
        let ift_program_id: Pubkey = abi_gmp
            .receiver
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid receiver as Solana pubkey: {e}"))?;

        // 3. Decode inner IFT payload: abi.encode(bytes32(receiver_pubkey), uint256(amount))
        let ift_decoded = ift_payload::decode_ift_mint_payload(&abi_gmp.payload)
            .context("Failed to decode inner IFT mint payload")?;

        tracing::debug!(
            receiver = %ift_decoded.receiver,
            amount = ift_decoded.amount,
            "Decoded IFT mint payload"
        );

        // 4. Find the mint from IFT bridge state on Solana
        let mint =
            find_ift_mint_for_client(&self.target_solana_client, &ift_program_id, dest_client)
                .context("Failed to find IFT mint for client")?;

        tracing::debug!(mint = %mint, "Found IFT mint for bridge");

        // 5. Resolve GMP program ID from the IBC app registration
        let gmp_program_id = self
            .resolve_port_program_id(GMP_PORT_ID)
            .context("Failed to resolve GMP program ID")?;

        // 6. Build the GmpSolanaPayload with correct accounts and instruction data
        let gmp_solana_payload = ift_payload::build_ift_mint_gmp_payload(
            &ift_decoded,
            &ift_payload::BuildIFTMintParams {
                ift_program_id,
                gmp_program_id,
                mint,
                dst_client_id: dest_client.to_string(),
                fee_payer: self.fee_payer,
            },
            &self.target_solana_client,
        )
        .context("Failed to build IFT mint GMP payload")?;

        // 7. Encode GmpSolanaPayload as protobuf bytes (for the hint account)
        let solana_payload_bytes = Protobuf::<solana_ibc_proto::RawGmpSolanaPayload>::encode_vec(
            gmp_solana_payload.clone(),
        );

        // 8. Build the store_payload_hint instruction
        let hint_pda = self.derive_hint_pda(gmp_program_id);
        let store_hint_ix =
            self.build_store_hint_instruction(gmp_program_id, hint_pda, &solana_payload_bytes);

        // 9. Build GMP account PDA from ABI-decoded fields
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
        let client_id = solana_ibc_types::ClientId::new(dest_client)
            .map_err(|e| anyhow::anyhow!("Invalid client ID: {e:?}"))?;

        let gmp_account =
            solana_ibc_types::GMPAccount::new(client_id, sender, salt, &gmp_program_id);
        let (gmp_account_pda, _) = gmp_account.pda();

        // 10. Build remaining accounts: [gmp_pda, target_program, hint_pda, execution_accounts...]
        let mut gmp_accounts = vec![
            AccountMeta {
                pubkey: gmp_account_pda,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: ift_program_id,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: hint_pda,
                is_signer: false,
                is_writable: false,
            },
        ];

        // Add execution accounts from GmpSolanaPayload
        for account_meta in &gmp_solana_payload.accounts {
            gmp_accounts.push(AccountMeta {
                pubkey: account_meta.pubkey,
                is_signer: false,
                is_writable: account_meta.is_writable,
            });
        }

        tracing::info!(
            gmp_pda = %gmp_account_pda,
            hint_pda = %hint_pda,
            num_accounts = gmp_accounts.len(),
            "Built ABI hint info"
        );

        Ok(AbiHintInfo {
            store_hint_instruction: store_hint_ix,
            gmp_accounts,
        })
    }

    /// Derive the payload hint PDA address.
    fn derive_hint_pda(&self, gmp_program_id: Pubkey) -> Pubkey {
        let (pda, _) = Pubkey::find_program_address(
            &[PAYLOAD_HINT_SEED, self.fee_payer.as_ref()],
            &gmp_program_id,
        );
        pda
    }

    /// Build the `store_payload_hint` instruction for the GMP program.
    fn build_store_hint_instruction(
        &self,
        gmp_program_id: Pubkey,
        hint_pda: Pubkey,
        payload_data: &[u8],
    ) -> Instruction {
        // Instruction data: discriminator + Borsh-encoded Vec<u8>
        let mut data = STORE_PAYLOAD_HINT_DISCRIMINATOR.to_vec();
        let len = u32::try_from(payload_data.len()).expect("payload_data length fits in u32");
        data.extend_from_slice(&len.to_le_bytes());
        data.extend_from_slice(payload_data);

        Instruction {
            program_id: gmp_program_id,
            accounts: vec![
                AccountMeta::new(hint_pda, false),
                AccountMeta::new(self.fee_payer, true),
                AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            ],
            data,
        }
    }
}

/// Partial deserialization of IFTBridge account to extract mint and client_id.
#[derive(AnchorDeserialize)]
struct IFTBridgePartial {
    _version: u8,
    _bump: u8,
    mint: Pubkey,
    client_id: String,
}

/// Find the SPL token mint associated with an IFT bridge for a given client ID.
fn find_ift_mint_for_client(
    solana_client: &Arc<RpcClient>,
    ift_program_id: &Pubkey,
    client_id: &str,
) -> Result<Pubkey> {
    tracing::debug!(
        %ift_program_id,
        %client_id,
        "Scanning IFT program accounts for bridge"
    );

    let accounts = solana_client
        .get_program_accounts(ift_program_id)
        .map_err(|e| anyhow::anyhow!("Failed to get IFT program accounts: {e}"))?;

    for (pubkey, account) in &accounts {
        if account.data.len() < 8 || account.data[..8] != *IFT_BRIDGE_DISCRIMINATOR {
            continue;
        }

        let mut data = &account.data[8..];
        let bridge = match IFTBridgePartial::deserialize(&mut data) {
            Ok(b) => b,
            Err(e) => {
                tracing::debug!(%pubkey, error = %e, "Failed to deserialize IFTBridge");
                continue;
            }
        };

        if bridge.client_id == client_id {
            tracing::debug!(mint = %bridge.mint, "Found IFT bridge");
            return Ok(bridge.mint);
        }
    }

    anyhow::bail!("No IFT bridge found for client '{client_id}' in program {ift_program_id}")
}
