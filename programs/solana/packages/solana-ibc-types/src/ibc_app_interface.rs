//! IBC App Interface
//!
//! This module defines the trait that all IBC applications must implement
//! to be compatible with the ICS26 router.
//!
//! By implementing this trait, apps ensure they have all required callback
//! functions with the correct signatures at compile time.

/// IBC app callback instruction names and discriminators
/// These MUST match the function names in your #[ibc_app] module
pub mod ibc_app_instructions {
    use anchor_lang::solana_program::hash::hash;

    /// Instruction name for receiving packets
    /// Your #[program] function MUST be named: `on_recv_packet`
    pub const ON_RECV_PACKET: &str = "on_recv_packet";

    /// Instruction name for acknowledgement callbacks
    /// Your #[program] function MUST be named: `on_acknowledgement_packet`
    pub const ON_ACKNOWLEDGEMENT_PACKET: &str = "on_acknowledgement_packet";

    /// Instruction name for timeout callbacks
    /// Your #[program] function MUST be named: `on_timeout_packet`
    pub const ON_TIMEOUT_PACKET: &str = "on_timeout_packet";

    fn compute_discriminator(instruction_name: &str) -> [u8; 8] {
        let preimage = format!("global:{instruction_name}");
        let mut hash_result = [0u8; 8];
        hash_result.copy_from_slice(&hash(preimage.as_bytes()).to_bytes()[..8]);
        hash_result
    }

    pub fn on_recv_packet_discriminator() -> [u8; 8] {
        compute_discriminator(ON_RECV_PACKET)
    }

    pub fn on_acknowledgement_packet_discriminator() -> [u8; 8] {
        compute_discriminator(ON_ACKNOWLEDGEMENT_PACKET)
    }

    pub fn on_timeout_packet_discriminator() -> [u8; 8] {
        compute_discriminator(ON_TIMEOUT_PACKET)
    }
}
