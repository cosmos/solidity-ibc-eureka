//! Alloy-generated Solidity types for IFT ABI encoding.

use alloy_sol_types::SolCall;

// Using ABI JSON because sol! macro can't resolve Solidity imports.
alloy_sol_types::sol!(IFT, "../../../../abi/IFTOwnable.json");

/// Construct ABI-encoded call to `iftMint(address, uint256)` for EVM chains.
pub fn encode_ift_mint_call(receiver: [u8; 20], amount: u64) -> Vec<u8> {
    use alloy_sol_types::private::{Address, U256};

    IFT::iftMintCall {
        receiver: Address::from(receiver),
        amount: U256::from(amount),
    }
    .abi_encode()
}
