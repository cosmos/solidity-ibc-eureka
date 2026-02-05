//! Tests for state module

use super::*;
use solana_sdk::pubkey::Pubkey;

#[test]
fn test_ift_app_state_seeds() {
    let mint = Pubkey::new_unique();
    let seeds = IFTAppState::seeds(&mint);

    assert_eq!(seeds.len(), 2);
    assert_eq!(seeds[0], IFT_APP_STATE_SEED.to_vec());
    assert_eq!(seeds[1], mint.as_ref().to_vec());
}

#[test]
fn test_ift_app_state_signer_seeds() {
    let mint = Pubkey::new_unique();
    let app_state = IFTAppState {
        version: AccountVersion::V1,
        bump: 255,
        mint,
        mint_authority_bump: 254,
        access_manager: Pubkey::new_unique(),
        gmp_program: Pubkey::new_unique(),
        daily_mint_limit: 0,
        rate_limit_day: 0,
        rate_limit_daily_usage: 0,
        paused: false,
        _reserved: [0; 128],
    };

    let signer_seeds = app_state.signer_seeds();

    assert_eq!(signer_seeds.len(), 3);
    assert_eq!(signer_seeds[0], IFT_APP_STATE_SEED.to_vec());
    assert_eq!(signer_seeds[1], mint.as_ref().to_vec());
    assert_eq!(signer_seeds[2], vec![255u8]);
}

#[test]
fn test_ift_bridge_seeds() {
    let mint = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    let seeds = IFTBridge::seeds(&mint, client_id);

    assert_eq!(seeds.len(), 3);
    assert_eq!(seeds[0], IFT_BRIDGE_SEED.to_vec());
    assert_eq!(seeds[1], mint.as_ref().to_vec());
    assert_eq!(seeds[2], client_id.as_bytes().to_vec());
}

#[test]
fn test_ift_bridge_seeds_empty_client_id() {
    let mint = Pubkey::new_unique();
    let seeds = IFTBridge::seeds(&mint, "");

    assert_eq!(seeds.len(), 3);
    assert_eq!(seeds[2], Vec::<u8>::new());
}

#[test]
fn test_pending_transfer_seeds() {
    let mint = Pubkey::new_unique();
    let client_id = "07-tendermint-0";
    let sequence = 42u64;
    let seeds = PendingTransfer::seeds(&mint, client_id, sequence);

    assert_eq!(seeds.len(), 4);
    assert_eq!(seeds[0], PENDING_TRANSFER_SEED.to_vec());
    assert_eq!(seeds[1], mint.as_ref().to_vec());
    assert_eq!(seeds[2], client_id.as_bytes().to_vec());
    assert_eq!(seeds[3], sequence.to_le_bytes().to_vec());
}

#[test]
fn test_pending_transfer_seeds_sequence_zero() {
    let mint = Pubkey::new_unique();
    let seeds = PendingTransfer::seeds(&mint, "client", 0);

    assert_eq!(seeds[3], 0u64.to_le_bytes().to_vec());
}

#[test]
fn test_pending_transfer_seeds_sequence_max() {
    let mint = Pubkey::new_unique();
    let seeds = PendingTransfer::seeds(&mint, "client", u64::MAX);

    assert_eq!(seeds[3], u64::MAX.to_le_bytes().to_vec());
}

#[test]
fn test_account_version_default() {
    let version = AccountVersion::default();
    assert_eq!(version, AccountVersion::V1);
}

#[test]
fn test_ift_bridge_serialization_roundtrip() {
    use anchor_lang::AccountDeserialize;

    let mint = Pubkey::new_unique();
    let bridge = IFTBridge {
        version: AccountVersion::V1,
        bump: 42,
        mint,
        client_id: "07-tendermint-0".to_string(),
        counterparty_ift_address: "0x1234567890abcdef".to_string(),
        chain_options: ChainOptions::Evm,
        active: true,
        _reserved: [0; 64],
    };

    // Serialize with discriminator
    let mut data = IFTBridge::DISCRIMINATOR.to_vec();
    bridge.serialize(&mut data).unwrap();

    // Deserialize
    let deserialized: IFTBridge =
        IFTBridge::try_deserialize(&mut &data[..]).expect("Failed to deserialize");

    assert_eq!(deserialized.version, AccountVersion::V1);
    assert_eq!(deserialized.bump, 42);
    assert_eq!(deserialized.mint, mint);
    assert_eq!(deserialized.client_id, "07-tendermint-0");
    assert_eq!(deserialized.counterparty_ift_address, "0x1234567890abcdef");
    assert!(matches!(deserialized.chain_options, ChainOptions::Evm));
    assert!(deserialized.active);
}
