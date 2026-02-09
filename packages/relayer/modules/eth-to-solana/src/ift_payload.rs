//! IFT payload translation: ABI-encoded (bytes32, uint256) → GmpSolanaPayload.
//!
//! EVM sends `abi.encode(bytes32(receiver), uint256(amount))`.
//! This module decodes it and builds a `GmpSolanaPayload` with the correct
//! accounts matching `ift_mint.rs`.

use std::sync::Arc;

use anchor_lang::prelude::*;
use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address;

use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use crate::proto::{GmpSolanaPayload, SolanaAccountMeta};

/// IFT PDA seeds (must match ift program)
const IFT_APP_STATE_SEED: &[u8] = b"ift_app_state";
const IFT_BRIDGE_SEED: &[u8] = b"ift_bridge";
const MINT_AUTHORITY_SEED: &[u8] = b"ift_mint_authority";

/// Decoded IFT mint payload from EVM ABI encoding.
#[derive(Debug, Clone)]
pub struct IFTMintPayload {
    /// Receiver Solana pubkey (decoded from bytes32)
    pub receiver: Pubkey,
    /// Amount to mint
    pub amount: u64,
}

/// Decode the ABI-encoded IFT payload from EVM.
///
/// EVM sends: `abi.encode(bytes32(receiver), uint256(amount))`
/// This is 64 bytes: 32 bytes receiver + 32 bytes amount (big-endian).
pub fn decode_ift_mint_payload(abi_payload: &[u8]) -> Result<IFTMintPayload> {
    if abi_payload.len() != 64 {
        anyhow::bail!(
            "IFT payload must be 64 bytes (bytes32 + uint256), got {}",
            abi_payload.len()
        );
    }

    // First 32 bytes: bytes32 receiver (Solana pubkey)
    let receiver_bytes: [u8; 32] = abi_payload[..32]
        .try_into()
        .context("Failed to extract receiver bytes")?;
    let receiver = Pubkey::from(receiver_bytes);

    // Next 32 bytes: uint256 amount (big-endian)
    let amount_bytes: [u8; 32] = abi_payload[32..64]
        .try_into()
        .context("Failed to extract amount bytes")?;

    // Verify the amount fits in u64 (upper 24 bytes must be zero)
    if amount_bytes[..24] != [0u8; 24] {
        anyhow::bail!("IFT amount exceeds u64 max");
    }

    let amount = u64::from_be_bytes(
        amount_bytes[24..32]
            .try_into()
            .context("Failed to convert amount to u64")?,
    );

    Ok(IFTMintPayload { receiver, amount })
}

/// Parameters for building the IFT mint GMP payload.
pub struct BuildIFTMintParams {
    /// The IFT program ID on Solana
    pub ift_program_id: Pubkey,
    /// The GMP program ID on Solana
    pub gmp_program_id: Pubkey,
    /// The mint address
    pub mint: Pubkey,
    /// Destination client ID (on Solana, tracking the source EVM chain)
    pub dst_client_id: String,
    /// Fee payer for the transaction
    pub fee_payer: Pubkey,
}

/// Build a `GmpSolanaPayload` for the IFT mint instruction.
///
/// This creates the payload that GMP will use to CPI into the IFT program.
/// Account order must match `IFTMint` in `ift_mint.rs`.
pub fn build_ift_mint_gmp_payload(
    decoded: &IFTMintPayload,
    params: &BuildIFTMintParams,
    solana_client: &Arc<RpcClient>,
) -> Result<GmpSolanaPayload> {
    let mint = params.mint;
    let ift_program_id = params.ift_program_id;

    // Derive PDAs
    let (app_state_pda, _) =
        Pubkey::find_program_address(&[IFT_APP_STATE_SEED, mint.as_ref()], &ift_program_id);

    let (ift_bridge_pda, _) = Pubkey::find_program_address(
        &[
            IFT_BRIDGE_SEED,
            mint.as_ref(),
            params.dst_client_id.as_bytes(),
        ],
        &ift_program_id,
    );

    let (mint_authority_pda, _) =
        Pubkey::find_program_address(&[MINT_AUTHORITY_SEED, mint.as_ref()], &ift_program_id);

    let receiver_token_account = get_associated_token_address(&decoded.receiver, &mint);

    // Derive GMP account PDA - the counterparty_ift_address is read from the bridge
    let counterparty_ift_address =
        read_bridge_counterparty_address(solana_client, &ift_bridge_pda)?;

    let client_id = solana_ibc_types::ClientId::new(&params.dst_client_id)
        .map_err(|e| anyhow::anyhow!("Invalid client ID: {e:?}"))?;

    let gmp_account = solana_ibc_types::GMPAccount::new(
        client_id,
        counterparty_ift_address
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid counterparty address for Sender"))?,
        solana_ibc_types::ics27::Salt::empty(),
        &params.gmp_program_id,
    );
    let (gmp_account_pda, _) = gmp_account.pda();

    // Borsh-encode the IFTMintMsg
    let mint_msg = IFTMintMsgBorsh {
        receiver: decoded.receiver,
        amount: decoded.amount,
    };
    let data = borsh::to_vec(&mint_msg).context("Failed to Borsh-encode IFTMintMsg")?;

    // Prepend the Anchor discriminator for `ift_mint`
    let discriminator = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"global:ift_mint");
        let result = hasher.finalize();
        <[u8; 8]>::try_from(&result[..8]).expect("sha256 output is at least 8 bytes")
    };

    let mut instruction_data = discriminator.to_vec();
    instruction_data.extend_from_slice(&data);

    // Build accounts matching IFTMint struct order in ift_mint.rs:
    // 0: app_state (writable)
    // 1: ift_bridge (readonly)
    // 2: mint (writable)
    // 3: mint_authority (readonly)
    // 4: receiver_token_account (writable)
    // 5: receiver_owner (readonly)
    // 6: gmp_program (readonly)
    // 7: gmp_account (signer via CPI)
    // 8: payer (writable, signer)
    // 9: token_program (readonly)
    // 10: associated_token_program (readonly)
    // 11: system_program (readonly)
    let accounts = vec![
        SolanaAccountMeta {
            pubkey: app_state_pda,
            is_signer: false,
            is_writable: true,
        },
        SolanaAccountMeta {
            pubkey: ift_bridge_pda,
            is_signer: false,
            is_writable: false,
        },
        SolanaAccountMeta {
            pubkey: mint,
            is_signer: false,
            is_writable: true,
        },
        SolanaAccountMeta {
            pubkey: mint_authority_pda,
            is_signer: false,
            is_writable: false,
        },
        SolanaAccountMeta {
            pubkey: receiver_token_account,
            is_signer: false,
            is_writable: true,
        },
        SolanaAccountMeta {
            pubkey: decoded.receiver,
            is_signer: false,
            is_writable: false,
        },
        SolanaAccountMeta {
            pubkey: params.gmp_program_id,
            is_signer: false,
            is_writable: false,
        },
        SolanaAccountMeta {
            pubkey: gmp_account_pda,
            is_signer: true,
            is_writable: false,
        },
        SolanaAccountMeta {
            pubkey: params.fee_payer,
            is_signer: true,
            is_writable: true,
        },
        SolanaAccountMeta {
            pubkey: spl_token::id(),
            is_signer: false,
            is_writable: false,
        },
        SolanaAccountMeta {
            pubkey: spl_associated_token_account::id(),
            is_signer: false,
            is_writable: false,
        },
        SolanaAccountMeta {
            pubkey: solana_sdk::system_program::id(),
            is_signer: false,
            is_writable: false,
        },
    ];

    // payer_position = 8 (index of the payer account)
    Ok(GmpSolanaPayload {
        data: instruction_data,
        accounts,
        payer_position: Some(8),
    })
}

/// Borsh-serializable IFTMintMsg matching the on-chain struct.
#[derive(AnchorSerialize, AnchorDeserialize)]
struct IFTMintMsgBorsh {
    pub receiver: Pubkey,
    pub amount: u64,
}

/// Read the counterparty IFT address from the IFTBridge account on-chain.
fn read_bridge_counterparty_address(
    solana_client: &Arc<RpcClient>,
    bridge_pda: &Pubkey,
) -> Result<String> {
    let account = solana_client
        .get_account_with_commitment(bridge_pda, CommitmentConfig::confirmed())
        .map_err(|e| anyhow::anyhow!("Failed to fetch IFTBridge account: {e}"))?
        .value
        .ok_or_else(|| anyhow::anyhow!("IFTBridge account not found at {bridge_pda}"))?;

    if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
        anyhow::bail!("IFTBridge account data too short");
    }

    // Deserialize IFTBridge (skip Anchor discriminator)
    let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];

    // Manual deserialization to extract counterparty_ift_address:
    // IFTBridge layout: version(1) + bump(1) + mint(32) + client_id(4+len) + counterparty_ift_address(4+len) + ...
    #[derive(AnchorDeserialize)]
    struct IFTBridgePartial {
        _version: u8,
        _bump: u8,
        _mint: Pubkey,
        _client_id: String,
        counterparty_ift_address: String,
    }

    let bridge = IFTBridgePartial::deserialize(&mut data)
        .context("Failed to deserialize IFTBridge account")?;

    Ok(bridge.counterparty_ift_address)
}

/// Read the mint and gmp_program from the IFTAppState account on-chain.
pub fn read_app_state(
    solana_client: &Arc<RpcClient>,
    ift_program_id: Pubkey,
    mint: Pubkey,
) -> Result<IFTAppStateInfo> {
    let (app_state_pda, _) =
        Pubkey::find_program_address(&[IFT_APP_STATE_SEED, mint.as_ref()], &ift_program_id);

    let account = solana_client
        .get_account_with_commitment(&app_state_pda, CommitmentConfig::confirmed())
        .map_err(|e| anyhow::anyhow!("Failed to fetch IFTAppState account: {e}"))?
        .value
        .ok_or_else(|| anyhow::anyhow!("IFTAppState account not found"))?;

    if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
        anyhow::bail!("IFTAppState account data too short");
    }

    let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];

    #[derive(AnchorDeserialize)]
    struct IFTAppStatePartial {
        _version: u8,
        _bump: u8,
        mint: Pubkey,
        _mint_authority_bump: u8,
        _admin: Pubkey,
        gmp_program: Pubkey,
    }

    let state = IFTAppStatePartial::deserialize(&mut data)
        .context("Failed to deserialize IFTAppState account")?;

    Ok(IFTAppStateInfo {
        mint: state.mint,
        gmp_program: state.gmp_program,
    })
}

/// Extracted info from IFTAppState.
pub struct IFTAppStateInfo {
    pub mint: Pubkey,
    pub gmp_program: Pubkey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_ift_mint_payload() {
        // Create a test payload: bytes32 receiver + uint256 amount
        let receiver = Pubkey::new_unique();
        let amount: u64 = 1_000_000;

        let mut payload = Vec::with_capacity(64);
        payload.extend_from_slice(&receiver.to_bytes()); // 32 bytes
        payload.extend_from_slice(&[0u8; 24]); // padding for uint256
        payload.extend_from_slice(&amount.to_be_bytes()); // 8 bytes

        let decoded = decode_ift_mint_payload(&payload).unwrap();
        assert_eq!(decoded.receiver, receiver);
        assert_eq!(decoded.amount, amount);
    }

    #[test]
    fn test_decode_ift_mint_payload_wrong_size() {
        let payload = vec![0u8; 63]; // too short
        assert!(decode_ift_mint_payload(&payload).is_err());
    }

    #[test]
    fn test_decode_ift_mint_payload_amount_overflow() {
        let mut payload = vec![0u8; 64];
        payload[0] = 1; // set some non-zero in upper bytes of amount area
                        // Actually this sets receiver byte, let's set amount overflow
        payload[32] = 1; // first byte of uint256 amount (upper 24 bytes must be 0)
        assert!(decode_ift_mint_payload(&payload).is_err());
    }
}
