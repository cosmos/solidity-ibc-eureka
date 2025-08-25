use thiserror::Error;

#[derive(Debug, Error)]
/// Collection of errors that can occur when
/// verifying packet memebership.
pub enum PacketAttestationError {
    #[error("verification failed: {reason}")]
    /// Packet cannot be deserialized from bytes
    VerificiationFailed {
        /// Reason for the failure
        reason: String,
    },
}
