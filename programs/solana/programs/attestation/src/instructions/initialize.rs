use crate::{error::ErrorCode, state::EthereumAddress, Initialize};
use anchor_lang::prelude::*;
use std::collections::HashSet;

pub fn handler(
    ctx: Context<Initialize>,
    _client_id: String,
    attestor_addresses: Vec<EthereumAddress>,
    min_required_sigs: u8,
    initial_height: u64,
    initial_timestamp: u64,
) -> Result<()> {
    // Validate attestor configuration
    require!(!attestor_addresses.is_empty(), ErrorCode::NoAttestors);
    require!(
        min_required_sigs > 0 && min_required_sigs <= attestor_addresses.len() as u8,
        ErrorCode::InvalidQuorum
    );

    // Check for duplicate attestor addresses
    let unique_addrs: HashSet<_> = attestor_addresses.iter().collect();
    require!(
        unique_addrs.len() == attestor_addresses.len(),
        ErrorCode::DuplicateAttestor
    );

    // Validate initial state
    require!(
        initial_height > 0 && initial_timestamp > 0,
        ErrorCode::InvalidState
    );

    // Initialize client state
    let client_state = &mut ctx.accounts.client_state;
    client_state.attestor_addresses = attestor_addresses;
    client_state.min_required_sigs = min_required_sigs;
    client_state.latest_height = initial_height;
    client_state.is_frozen = false;

    // Initialize initial consensus state
    let consensus_state = &mut ctx.accounts.initial_consensus_state;
    consensus_state.height = initial_height;
    consensus_state.timestamp = initial_timestamp;

    msg!(
        "Attestation light client initialized: {} attestors, quorum: {}, height: {}, timestamp: {}",
        client_state.attestor_addresses.len(),
        min_required_sigs,
        initial_height,
        initial_timestamp
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate_detection() {
        let addr1 = [1u8; 20];
        let addr2 = [2u8; 20];

        let unique_addrs: HashSet<_> = vec![addr1, addr2, addr1].into_iter().collect();
        assert_eq!(unique_addrs.len(), 2); // Should detect duplicate
    }
}
