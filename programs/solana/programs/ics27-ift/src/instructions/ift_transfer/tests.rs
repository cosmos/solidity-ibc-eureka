//! Tests for IFT transfer payload construction

use super::*;
use crate::evm_selectors::{IFT_MINT_DISCRIMINATOR, IFT_MINT_SELECTOR};

#[test]
fn test_hex_to_bytes_valid() {
    assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
    assert_eq!(hex_to_bytes("00").unwrap(), vec![0]);
    assert_eq!(hex_to_bytes("ff").unwrap(), vec![255]);
    assert_eq!(hex_to_bytes("FF").unwrap(), vec![255]);
    assert_eq!(
        hex_to_bytes("deadbeef").unwrap(),
        vec![0xde, 0xad, 0xbe, 0xef]
    );
    assert_eq!(
        hex_to_bytes("0123456789abcdef").unwrap(),
        vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
    );
}

#[test]
fn test_hex_to_bytes_invalid_odd_length() {
    assert!(hex_to_bytes("0").is_err());
    assert!(hex_to_bytes("abc").is_err());
    assert!(hex_to_bytes("12345").is_err());
}

#[test]
fn test_hex_to_bytes_invalid_chars() {
    assert!(hex_to_bytes("gg").is_err());
    assert!(hex_to_bytes("0x").is_err());
    assert!(hex_to_bytes("zz").is_err());
    assert!(hex_to_bytes("ab cd").is_err());
}

#[test]
fn test_construct_evm_mint_call_basic() {
    let receiver = "0x1234567890abcdef1234567890abcdef12345678";
    let amount = 1_000_000u64;

    let payload = construct_evm_mint_call(receiver, amount).unwrap();

    // Should be 4 (selector) + 32 (address) + 32 (amount) = 68 bytes
    assert_eq!(payload.len(), 68);
    assert_eq!(&payload[0..4], &IFT_MINT_SELECTOR);

    // Address should be left-padded to 32 bytes (12 zero bytes + 20 address bytes)
    assert_eq!(&payload[4..16], &[0u8; 12]);

    // Amount should be in last 32 bytes, big-endian, left-padded
    let amount_bytes = &payload[36..68];
    assert_eq!(&amount_bytes[0..24], &[0u8; 24]);
    assert_eq!(&amount_bytes[24..32], &amount.to_be_bytes());
}

#[test]
fn test_construct_evm_mint_call_without_0x_prefix() {
    let receiver = "1234567890abcdef1234567890abcdef12345678";
    let payload = construct_evm_mint_call(receiver, 500).unwrap();
    assert_eq!(payload.len(), 68);
}

#[test]
fn test_construct_evm_mint_call_max_amount() {
    let receiver = "0xffffffffffffffffffffffffffffffffffffffff";
    let payload = construct_evm_mint_call(receiver, u64::MAX).unwrap();
    let amount_bytes = &payload[36..68];
    assert_eq!(&amount_bytes[24..32], &u64::MAX.to_be_bytes());
}

#[test]
fn test_construct_evm_mint_call_invalid_hex() {
    assert!(construct_evm_mint_call("0xnothex", 100).is_err());
}

#[test]
fn test_construct_evm_mint_call_short_address() {
    let payload = construct_evm_mint_call("0xabcd", 100).unwrap();
    assert_eq!(payload.len(), 68);
    assert_eq!(&payload[4..34], &[0u8; 30]);
}

#[test]
fn test_construct_cosmos_mint_call() {
    let payload = construct_cosmos_mint_call("uatom", "cosmos1abc123", 1_000_000);
    let json_str = String::from_utf8(payload).unwrap();

    assert!(json_str.contains("\"@type\":\"/cosmos.ift.v1.MsgIFTMint\""));
    assert!(json_str.contains("\"denom\":\"uatom\""));
    assert!(json_str.contains("\"receiver\":\"cosmos1abc123\""));
    assert!(json_str.contains("\"amount\":\"1000000\""));
}

#[test]
fn test_construct_cosmos_mint_call_with_ibc_denom() {
    let payload = construct_cosmos_mint_call("ibc/ABC123", "cosmos1xyz", 42);
    let json_str = String::from_utf8(payload).unwrap();
    assert!(json_str.contains("\"denom\":\"ibc/ABC123\""));
}

#[test]
fn test_construct_solana_mint_call() {
    let receiver = "11111111111111111111111111111111";
    let amount = 999u64;

    let payload = construct_solana_mint_call(receiver, amount);

    assert_eq!(payload.len(), 8 + receiver.len() + 8);
    assert_eq!(&payload[0..8], &IFT_MINT_DISCRIMINATOR);

    let amount_start = payload.len() - 8;
    assert_eq!(&payload[amount_start..], &amount.to_le_bytes());
}

#[test]
fn test_construct_mint_call_evm() {
    let result = construct_mint_call(
        CounterpartyChainType::Evm,
        "ignored",
        "0x1234567890abcdef1234567890abcdef12345678",
        100,
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 68);
}

#[test]
fn test_construct_mint_call_cosmos() {
    let result = construct_mint_call(
        CounterpartyChainType::Cosmos,
        "uatom",
        "cosmos1receiver",
        100,
    );
    assert!(result.is_ok());
    let json = String::from_utf8(result.unwrap()).unwrap();
    assert!(json.contains("MsgIFTMint"));
}

#[test]
fn test_construct_mint_call_solana() {
    let result = construct_mint_call(
        CounterpartyChainType::Solana,
        "ignored",
        "SomeBase58Pubkey",
        100,
    );
    assert!(result.is_ok());
}
