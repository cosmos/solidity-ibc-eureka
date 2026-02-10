//! Payload translation: ABI (EVM) → Protobuf (Solana).
//!
//! When relaying from Ethereum to Solana, packet payloads arrive with
//! `encoding = "application/x-solidity-abi"` and ABI-encoded values.
//! Solana's GMP program expects `encoding = "application/x-protobuf"` with
//! protobuf-encoded `GmpPacketData`.
//!
//! This module translates ABI payloads into protobuf for the IFT (Inter-chain
//! Fungible Token) use case.

use std::sync::{Arc, LazyLock};

use alloy::sol_types::SolValue;
use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use crate::gmp::{GMP_PORT_ID, PROTOBUF_ENCODING};
use crate::ift_payload;
use crate::proto::{GmpPacketData, Protobuf};

use super::SolanaTxBuilder;

/// ABI encoding identifier used by Ethereum ICS27 GMP.
const ABI_ENCODING: &str = "application/x-solidity-abi";

/// Anchor discriminator for `IFTBridge` accounts.
static IFT_BRIDGE_DISCRIMINATOR: LazyLock<[u8; 8]> = LazyLock::new(|| {
    let mut hasher = Sha256::new();
    hasher.update(b"account:ift::IFTBridge");
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

impl SolanaTxBuilder {
    /// Translate ABI-encoded payloads in a `MsgRecvPacket` to protobuf for Solana.
    ///
    /// For each payload with `encoding = "application/x-solidity-abi"`:
    /// 1. ABI-decode the outer `GMPPacketData`
    /// 2. Decode the inner IFT mint payload (`bytes32 receiver + uint256 amount`)
    /// 3. Build a `GmpSolanaPayload` with proper accounts and Borsh instruction data
    /// 4. Re-encode as protobuf `GmpPacketData`
    /// 5. Update the encoding field to `"application/x-protobuf"`
    pub fn translate_evm_recv_msg(
        &self,
        msg: &mut ibc_proto_eureka::ibc::core::channel::v2::MsgRecvPacket,
    ) -> Result<()> {
        let packet = match msg.packet.as_mut() {
            Some(p) => p,
            None => return Ok(()),
        };

        let dest_client = packet.destination_client.clone();

        for payload in &mut packet.payloads {
            if payload.encoding != ABI_ENCODING {
                continue;
            }

            if payload.destination_port != GMP_PORT_ID {
                tracing::debug!(
                    dest_port = %payload.destination_port,
                    "Skipping ABI payload translation for non-GMP port"
                );
                continue;
            }

            self.translate_single_payload(payload, &dest_client)?;
        }

        Ok(())
    }

    /// Translate a single ABI-encoded payload to protobuf.
    fn translate_single_payload(
        &self,
        payload: &mut ibc_proto_eureka::ibc::core::channel::v2::Payload,
        dest_client: &str,
    ) -> Result<()> {
        tracing::info!("Translating ABI payload to protobuf for dest_client={dest_client}");

        // 1. ABI-decode the outer GMPPacketData
        let abi_gmp: AbiGmpPacketData = SolValue::abi_decode(&payload.value)
            .map_err(|e| anyhow::anyhow!("Failed to ABI decode GMPPacketData: {e}"))?;

        tracing::debug!(
            sender = %abi_gmp.sender,
            receiver = %abi_gmp.receiver,
            payload_len = abi_gmp.payload.len(),
            "Decoded ABI GMPPacketData"
        );

        // 2. Parse receiver as the target program ID (IFT program for IFT transfers)
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

        // 7. Encode GmpSolanaPayload as protobuf bytes
        let solana_payload_bytes =
            Protobuf::<solana_ibc_proto::RawGmpSolanaPayload>::encode_vec(gmp_solana_payload);

        // 8. Build new protobuf GmpPacketData with translated payload
        let proto_gmp = GmpPacketData {
            sender: abi_gmp
                .sender
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid sender"))?,
            receiver: abi_gmp
                .receiver
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid receiver"))?,
            salt: abi_gmp
                .salt
                .to_vec()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid salt"))?,
            payload: solana_payload_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Empty payload"))?,
            memo: abi_gmp
                .memo
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid memo"))?,
        };

        // 9. Encode as protobuf and update
        let proto_bytes = Protobuf::<solana_ibc_proto::RawGmpPacketData>::encode_vec(proto_gmp);

        payload.value = proto_bytes;
        payload.encoding = PROTOBUF_ENCODING.to_string();

        tracing::info!("Successfully translated ABI payload to protobuf");

        Ok(())
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
///
/// Scans all accounts owned by the IFT program, looking for bridge accounts
/// whose `client_id` matches the destination client.
fn find_ift_mint_for_client(
    solana_client: &Arc<RpcClient>,
    ift_program_id: &Pubkey,
    client_id: &str,
) -> Result<Pubkey> {
    let accounts = solana_client
        .get_program_accounts_with_config(
            ift_program_id,
            solana_client::rpc_config::RpcProgramAccountsConfig {
                account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map_err(|e| anyhow::anyhow!("Failed to get IFT program accounts: {e}"))?;

    for (_pubkey, account) in &accounts {
        if account.data.len() < 8 || account.data[..8] != *IFT_BRIDGE_DISCRIMINATOR {
            continue;
        }

        let mut data = &account.data[8..];
        let bridge = match IFTBridgePartial::deserialize(&mut data) {
            Ok(b) => b,
            Err(_) => continue,
        };

        if bridge.client_id == client_id {
            tracing::debug!(
                mint = %bridge.mint,
                client_id = %bridge.client_id,
                "Found matching IFT bridge"
            );
            return Ok(bridge.mint);
        }
    }

    anyhow::bail!("No IFT bridge found for client '{client_id}' in program {ift_program_id}")
}
