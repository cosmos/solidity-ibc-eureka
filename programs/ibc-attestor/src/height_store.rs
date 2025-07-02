use std::collections::VecDeque;

use crate::attestation::Attestation;

pub struct AttestationStore {
    store: VecDeque<(u64, Attestation)>,
    max_entries: usize,
}

const NINTEY_SECS: u64 = 90_000;

impl AttestationStore {
    pub fn new(block_time_ms: u64) -> Self {
        let max_entries = NINTEY_SECS / block_time_ms;

        Self {
            store: VecDeque::with_capacity(max_entries as usize),
            max_entries: max_entries as usize,
        }
    }

    /// Add a new attestation to a specific height
    pub fn push(&mut self, height: u64, value: Attestation) {
        if self.store.back().is_some_and(|(h, _)| h == &height) {
            tracing::info!("value at this height already in store");
            return;
        }

        if self.store.len() == self.max_entries {
            tracing::info!("popping oldest entry");
            self.store.pop_front();
        }
        self.store.push_back((height, value));
    }

    /// Get an iterator over all attestations from a given height
    pub fn range_from<'a>(
        &'a self,
        height: u64,
    ) -> impl Iterator<Item = &'a (u64, Attestation)> + 'a {
        self.store.iter().filter(move |(k, _)| k >= &height)
    }
}
