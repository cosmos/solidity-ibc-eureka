use std::collections::VecDeque;

use crate::attestation::Attestation;
use crate::attestor::AttestorConfig;

pub struct HeightStore {
    store: VecDeque<(u64, Attestation)>,
    max_entries: usize,
}

impl HeightStore {
    pub fn from_config(config: &AttestorConfig) -> Self {
        Self {
            store: VecDeque::with_capacity(config.max_entries as usize),
            max_entries: config.max_entries as usize,
        }
    }

    /// Add a new attestation to a specific height
    pub fn push(&mut self, key: u64, value: Attestation) {
        if self.store.len() == self.max_entries {
            self.store.pop_front();
        }
        self.store.push_back((key, value));
    }

    /// Get an iterator over all attestations from a given height
    pub fn range_from<'a>(
        &'a self,
        height: u64,
    ) -> impl Iterator<Item = &'a (u64, Attestation)> + 'a {
        self.store.iter().filter(move |(k, _)| k >= &height)
    }
}
