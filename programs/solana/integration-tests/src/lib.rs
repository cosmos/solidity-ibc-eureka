use solana_sdk::pubkey::Pubkey;

pub mod accounts;
pub mod chain;
pub mod gmp;
pub mod relayer;
pub mod router;
pub mod user;

/// Shared interface for test actors (`User`, `Relayer`).
pub trait Actor {
    fn pubkey(&self) -> Pubkey;
}
