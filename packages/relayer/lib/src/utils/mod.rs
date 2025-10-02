//! This module contains the utilities for relayer implementations.

use futures_timer::Delay;
use std::future::Future;
use std::time::{Duration, Instant};

/// Retries an operation until the condition is met or a timeout occurs.
///
/// The basic version just checks for a boolean condition.
///
/// # Errors
/// If the condition is not met within the timeout, an error is returned.
pub async fn wait_for_condition<F, Fut>(
    timeout: Duration,
    interval: Duration,
    mut condition: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut + Send,
    Fut: Future<Output = anyhow::Result<bool>> + Send,
{
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition().await? {
            return Ok(());
        }

        tracing::debug!(
            "Condition not met. Waiting for {} seconds before retrying",
            interval.as_secs()
        );
        Delay::new(interval).await;
    }
    anyhow::bail!("Timeout exceeded waiting for condition")
}

/// Converts to exactly 32 bytes - panics if not exactly 32 bytes
///
/// # Panics
/// Panics if the input bytes are not exactly 32 bytes in length
#[must_use]
pub fn to_32_bytes_exact(bytes: &[u8], field_name: &str) -> [u8; 32] {
    bytes.try_into().unwrap_or_else(|_| {
        panic!(
            "{} must be exactly 32 bytes, got {} bytes",
            field_name,
            bytes.len()
        )
    })
}

/// Converts to 32 bytes with padding - pads with zeros if less, panics if more
///
/// # Panics
/// Panics if the input bytes exceed 32 bytes in length
#[must_use]
pub fn to_32_bytes_padded(bytes: &[u8], field_name: &str) -> [u8; 32] {
    match bytes.len() {
        0..=32 => {
            let mut arr = [0u8; 32];
            arr[..bytes.len()].copy_from_slice(bytes);
            arr
        }
        _ => panic!("{} exceeds 32 bytes: got {} bytes", field_name, bytes.len()),
    }
}

pub mod cosmos;
pub mod eth_eureka;
pub mod solana_eureka;
