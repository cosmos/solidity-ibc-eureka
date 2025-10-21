use anchor_lang::prelude::*;

/// Global counter app state
#[account]
pub struct CounterAppState {
    /// Authority that can manage the app
    pub authority: Pubkey,
    /// Total number of user counters created
    pub total_counters: u64,
    /// Total number of GMP calls processed
    pub total_gmp_calls: u64,
    /// Program bump seed
    pub bump: u8,
}

impl CounterAppState {
    pub const SEED: &'static [u8] = b"counter_app_state";
    pub const INIT_SPACE: usize = 8 + // discriminator
        32 + // authority
        8 + // total_counters
        8 + // total_gmp_calls
        1; // bump
}

/// Per-user counter state
#[account]
pub struct UserCounter {
    /// User's public key
    pub user: Pubkey,
    /// Current counter value
    pub count: u64,
    /// Number of increments
    pub increments: u64,
    /// Number of decrements
    pub decrements: u64,
    /// Last updated timestamp
    pub last_updated: i64,
    /// PDA bump seed
    pub bump: u8,
}

impl UserCounter {
    pub const SEED: &'static [u8] = b"user_counter";
    pub const INIT_SPACE: usize = 8 + // discriminator
        32 + // user
        8 + // count
        8 + // increments
        8 + // decrements
        8 + // last_updated
        1; // bump

    pub fn increment(&mut self, amount: u64, current_time: i64) -> Result<()> {
        self.count = self
            .count
            .checked_add(amount)
            .ok_or(crate::errors::CounterError::CounterOverflow)?;
        self.increments = self.increments.saturating_add(1);
        self.last_updated = current_time;
        Ok(())
    }

    pub fn decrement(&mut self, amount: u64, current_time: i64) -> Result<()> {
        self.count = self
            .count
            .checked_sub(amount)
            .ok_or(crate::errors::CounterError::CounterUnderflow)?;
        self.decrements = self.decrements.saturating_add(1);
        self.last_updated = current_time;
        Ok(())
    }
}

/// GMP callback data for tracking calls
#[account]
pub struct GMPCallState {
    /// User who initiated the call
    pub user: Pubkey,
    /// Original payload hash
    pub payload_hash: [u8; 32],
    /// Call timestamp
    pub timestamp: i64,
    /// Success status
    pub success: bool,
    /// PDA bump seed
    pub bump: u8,
}

impl GMPCallState {
    pub const SEED: &'static [u8] = b"gmp_call_state";
    pub const INIT_SPACE: usize = 8 + // discriminator
        32 + // user
        32 + // payload_hash
        8 + // timestamp
        1 + // success
        1; // bump
}
