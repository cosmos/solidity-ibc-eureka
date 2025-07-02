//! An in-memory implementation of a [Attestation]
//! store.
use std::collections::VecDeque;

use crate::attestation::Attestation;

pub struct AttestationStore {
    store: VecDeque<(u64, Attestation)>,
    max_entries: usize,
}

const NINTEY_SECS: u64 = 90_000;

impl AttestationStore {
    /// Create a new store with a dynamic capacity
    /// that is determined by how many blocks can
    /// be created within 90 seconds.
    pub fn new(block_time_ms: u64) -> Self {
        let max_entries = NINTEY_SECS / block_time_ms;

        Self {
            store: VecDeque::with_capacity(max_entries as usize),
            max_entries: max_entries as usize,
        }
    }

    /// Add a new attestation at a specific height. Assumes
    /// monotonically increasing heights. Skips duplicate
    /// heights.
    pub fn push(&mut self, height: u64, value: Attestation) {
        if self.store.back().is_some_and(|(h, _)| h == &height) {
            tracing::debug!("value at this height already in store");
            return;
        }

        if self.store.len() == self.max_entries {
            tracing::debug!("popping oldest entry");
            self.store.pop_front();
        }
        self.store.push_back((height, value));
    }

    pub fn range_from<'a>(
        &'a self,
        height: u64,
    ) -> impl Iterator<Item = &'a (u64, Attestation)> + 'a {
        self.store.iter().filter(move |(k, _)| k >= &height)
    }
}

#[cfg(test)]
mod constructor {
    use super::*;

    #[test]
    fn calcs_max_entries_correctly() {
        let store = AttestationStore::new(9_000);
        assert_eq!(store.max_entries, 10);
    }
}

#[cfg(test)]
mod push {
    use super::*;

    #[test]
    fn does_not_add_duplicate_heights_but_adds_new_height() {
        let mut store = AttestationStore::new(9_000);

        for i in 1..=10 {
            let att = Attestation {
                signature: [0; 64],
                data: Vec::new(),
            };
            store.push(i, att);
        }

        assert_eq!(store.store.iter().count(), 10);

        store.push(
            10,
            Attestation {
                signature: [0; 64],
                data: Vec::new(),
            },
        );
        store.push(
            10,
            Attestation {
                signature: [0; 64],
                data: Vec::new(),
            },
        );
        assert_eq!(store.store.iter().count(), 10);
        assert_eq!(store.store.back().map(|(h, _)| h), Some(&10));

        store.push(
            11,
            Attestation {
                signature: [0; 64],
                data: Vec::new(),
            },
        );
        assert_eq!(store.store.iter().count(), 10);
        assert_eq!(store.store.back().map(|(h, _)| h), Some(&11));
    }
}

#[cfg(test)]
mod range_from {
    use super::*;

    #[test]
    fn returns_all_heights() {
        let mut store = AttestationStore::new(9_000);

        for i in 1..=10 {
            let att = Attestation {
                signature: [0; 64],
                data: Vec::new(),
            };
            store.push(i, att);
        }
        let range = store.range_from(0);

        assert_eq!(range.count(), 10);
    }

    #[test]
    fn returns_half_of_all_heights() {
        let mut store = AttestationStore::new(9_000);

        for i in 1..=10 {
            let att = Attestation {
                signature: [0; 64],
                data: Vec::new(),
            };
            store.push(i, att);
        }
        let range = store.range_from(6);

        assert_eq!(range.count(), 5);
    }

    #[test]
    fn returns_latest_height() {
        let mut store = AttestationStore::new(9_000);

        for i in 1..=10 {
            let att = Attestation {
                signature: [0; 64],
                data: Vec::new(),
            };
            store.push(i, att);
        }
        let range = store.range_from(10);

        assert_eq!(range.count(), 1);
    }

    #[test]
    fn no_heights() {
        let mut store = AttestationStore::new(9_000);

        for i in 1..=10 {
            let att = Attestation {
                signature: [0; 64],
                data: Vec::new(),
            };
            store.push(i, att);
        }
        let range = store.range_from(11);

        assert_eq!(range.count(), 0);
    }
}
