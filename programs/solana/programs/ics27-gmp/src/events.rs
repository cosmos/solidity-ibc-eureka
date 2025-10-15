use anchor_lang::prelude::*;

/// Event emitted when GMP app is initialized
#[event]
pub struct GMPAppInitialized {
    /// Router program managing this app
    pub router_program: Pubkey,
    /// Administrative authority
    pub authority: Pubkey,
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
    /// Target program to execute
    pub receiver: Pubkey,
    /// Source client ID
    pub client_id: String,
    /// Account salt used
    pub salt: Vec<u8>,
    /// Payload size
    pub payload_size: u64,
    /// Timeout timestamp
    pub timeout_timestamp: i64,
}

/// Event emitted when a packet is received and executed
#[event]
pub struct GMPExecutionCompleted {
    /// Account that executed the call
    pub account: Pubkey,
    /// Target program that was called
    pub target_program: Pubkey,
    /// Client ID
    pub client_id: String,
    /// Original sender
    pub sender: String,
    /// Account nonce after execution
    pub nonce: u64,
    /// Whether execution succeeded
    pub success: bool,
    /// Result data size
    pub result_size: u64,
    /// Execution timestamp
    pub timestamp: i64,
}

/// Event emitted when a new account is created
#[event]
pub struct GMPAccountCreated {
    /// Account address (PDA)
    pub account: Pubkey,
    /// Client ID
    pub client_id: String,
    /// Original sender
    pub sender: String,
    /// Salt used for derivation
    pub salt: Vec<u8>,
    /// Creation timestamp
    pub created_at: i64,
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

/// Event emitted when packet acknowledgement is processed
#[event]
pub struct GMPAcknowledgementProcessed {
    /// Original sender
    pub sender: Pubkey,
    /// Packet sequence
    pub sequence: u64,
    /// Whether acknowledgement indicates success
    pub ack_success: bool,
    /// Processing timestamp
    pub timestamp: i64,
}

/// Event emitted when packet timeout is processed
#[event]
pub struct GMPTimeoutProcessed {
    /// Original sender
    pub sender: Pubkey,
    /// Packet sequence
    pub sequence: u64,
    /// Timeout height or timestamp
    pub timeout_info: String,
    /// Processing timestamp
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

/// Event emitted when router caller PDA is created
#[event]
pub struct RouterCallerCreated {
    /// Router caller PDA address
    pub router_caller: Pubkey,
    /// PDA bump seed
    pub bump: u8,
}
