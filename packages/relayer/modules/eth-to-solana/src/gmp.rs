//! GMP account extraction for Eth→Solana relay.
//!
//! Handles both protobuf-encoded (Cosmos-originated) and ABI-encoded
//! (Ethereum-originated) GMP payloads. Decodes packet data and builds
//! the remaining accounts needed for Solana `recv_packet` transactions.

use alloy::sol_types::SolValue;
use anyhow::Result;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

pub use ibc_eureka_relayer_lib::utils::solana_gmp::{
    extract_gmp_accounts, find_gmp_result_pda, GMP_PORT_ID, PROTOBUF_ENCODING,
};

/// ABI encoding identifier used by Ethereum ICS27 GMP.
pub const ABI_ENCODING: &str = "application/x-solidity-abi";

/// Packed account entry size: pubkey(32) + `is_signer`(1) + `is_writable`(1)
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

/// Extract GMP remaining accounts from an ABI-encoded payload.
///
/// Decodes the outer `GMPPacketData` and inner `GmpSolanaPayload`, then builds
/// the account list: `[gmp_pda, target_program, execution_accounts...]`.
pub fn extract_abi_gmp_accounts(
    payload_value: &[u8],
    dest_client: &str,
    gmp_program_id: Pubkey,
) -> Result<Vec<AccountMeta>> {
    let abi_gmp: AbiGmpPacketData = SolValue::abi_decode(payload_value)
        .map_err(|e| anyhow::anyhow!("Failed to ABI decode GMPPacketData: {e}"))?;

    tracing::debug!(
        sender = %abi_gmp.sender,
        receiver = %abi_gmp.receiver,
        payload_len = abi_gmp.payload.len(),
        "Decoded ABI GMPPacketData"
    );

    let target_program: Pubkey = abi_gmp
        .receiver
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid receiver as Solana pubkey: {e}"))?;

    // Use abi_decode_params (not abi_decode) because constructMintCall returns
    // abi.encode(bytes, bytes, uint32) — three separate params without an outer
    // tuple offset.
    let abi_solana: AbiGmpSolanaPayload = SolValue::abi_decode_params(&abi_gmp.payload)
        .map_err(|e| anyhow::anyhow!("Failed to ABI decode GmpSolanaPayload: {e}"))?;

    tracing::debug!(
        packed_accounts_len = abi_solana.packedAccounts.len(),
        instruction_data_len = abi_solana.instructionData.len(),
        payer_position = abi_solana.payerPosition,
        "Decoded ABI GmpSolanaPayload"
    );

    let packed = &abi_solana.packedAccounts;
    if packed.len() % PACKED_ACCOUNT_SIZE != 0 {
        anyhow::bail!(
            "Packed accounts length {} is not a multiple of {}",
            packed.len(),
            PACKED_ACCOUNT_SIZE
        );
    }

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

    let mut accounts = vec![
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

    for chunk in packed.chunks_exact(PACKED_ACCOUNT_SIZE) {
        let pubkey_bytes: [u8; 32] = chunk[..32]
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid pubkey in packed accounts"))?;
        let is_writable = chunk[33] != 0;

        accounts.push(AccountMeta {
            pubkey: Pubkey::from(pubkey_bytes),
            is_signer: false,
            is_writable,
        });
    }

    tracing::info!(
        gmp_pda = %gmp_account_pda,
        num_accounts = accounts.len(),
        "Extracted ABI GMP accounts"
    );

    Ok(accounts)
}
