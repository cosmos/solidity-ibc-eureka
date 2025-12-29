pub mod initialize;
pub mod verify_membership;
pub mod verify_non_membership;

// TODO: CRITICAL - Add update_client module
// Missing module for the update_client instruction which corresponds to
// updateClient() in the Solidity implementation.
// This should be in a new file: src/instructions/update_client.rs
// See: contracts/light-clients/attestation/AttestationLightClient.sol:88-122
// pub mod update_client;
