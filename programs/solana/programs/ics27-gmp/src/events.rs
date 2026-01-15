use anchor_lang::prelude::*;

/// Event emitted when GMP app is initialized
#[event]
pub struct GMPAppInitialized {
    /// Router program managing this app
    pub router_program: Pubkey,
    /// Port ID bound to this app
    pub port_id: String,
    /// App initialization timestamp
    pub timestamp: i64,
}

/// Event emitted when a GMP call is sent
#[event]
pub struct GMPCallSent {
    /// Packet sequence number
    pub sequence: u64,
    /// Sender of the call
    pub sender: Pubkey,
    /// Target address to execute (destination chain format)
    pub receiver: String,
    /// Source client ID
    pub client_id: String,
    /// Account salt used
    pub salt: Vec<u8>,
    /// Payload size
    pub payload_size: u64,
    /// Timeout timestamp
    pub timeout_timestamp: i64,
}

/// Event emitted when app is paused
#[event]
pub struct GMPAppPaused {
    /// Admin who paused the app
    pub admin: Pubkey,
    /// Pause timestamp
    pub timestamp: i64,
}

/// Event emitted when app is unpaused
#[event]
pub struct GMPAppUnpaused {
    /// Admin who unpaused the app
    pub admin: Pubkey,
    /// Unpause timestamp
    pub timestamp: i64,
}

/// Event emitted for execution failures
#[event]
pub struct GMPExecutionFailed {
    /// Account that failed execution
    pub account: Pubkey,
    /// Target program that failed
    pub target_program: Pubkey,
    /// Error code
    pub error_code: u32,
    /// Error message
    pub error_message: String,
    /// Failure timestamp
    pub timestamp: i64,
}

#[event]
pub struct GMPCallAcknowledged {
    pub source_client: String,
    pub sequence: u64,
    pub sender: String,
    pub result_pda: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct GMPCallTimeout {
    pub source_client: String,
    pub sequence: u64,
    pub sender: String,
    pub result_pda: Pubkey,
    pub timestamp: i64,
}
