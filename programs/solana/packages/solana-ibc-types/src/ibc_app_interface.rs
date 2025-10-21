//! IBC App Interface
//!
//! This module defines the trait that all IBC applications must implement
//! to be compatible with the ICS26 router.
//!
//! By implementing this trait, apps ensure they have all required callback
//! functions with the correct signatures at compile time.

/// Standard instruction names for IBC app callbacks
/// These MUST match the function names in your #[ibc_app] module
pub mod instruction_names {
    /// Instruction name for receiving packets
    /// Your #[program] function MUST be named: `on_recv_packet`
    pub const ON_RECV_PACKET: &str = "global:on_recv_packet";

    /// Instruction name for acknowledgement callbacks
    /// Your #[program] function MUST be named: `on_acknowledgement_packet`
    pub const ON_ACKNOWLEDGEMENT_PACKET: &str = "global:on_acknowledgement_packet";

    /// Instruction name for timeout callbacks
    /// Your #[program] function MUST be named: `on_timeout_packet`
    pub const ON_TIMEOUT_PACKET: &str = "global:on_timeout_packet";
}
