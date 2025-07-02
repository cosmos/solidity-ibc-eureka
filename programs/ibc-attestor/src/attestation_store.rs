use std::collections::VecDeque;

use indexmap::IndexMap;

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

    fn range_from<'a>(&'a self, height: u64) -> impl Iterator<Item = &'a (u64, Attestation)> + 'a {
        self.store.iter().filter(move |(k, _)| k >= &height)
    }

    /// Returns a an [IndexMap] that contains all attestations
    /// in insertion order from a given `height`
    pub fn attestations_from_height(&self, height: u64) -> IndexMap<u64, Attestation> {
        let mut heights = IndexMap::new();
        for (h, v) in self.range_from(height) {
            heights.insert(h.clone(), v.clone());
        }
        heights
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

        assert_eq!(store.store.len(), 10);

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
        assert_eq!(store.store.len(), 10);
        assert_eq!(store.store.back().map(|(h, _)| h), Some(&10));

        store.push(
            11,
            Attestation {
                signature: [0; 64],
                data: Vec::new(),
            },
        );
        assert_eq!(store.store.len(), 10);
        assert_eq!(store.store.back().map(|(h, _)| h), Some(&11));
    }
}

#[cfg(test)]
mod attestations_from_height {
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
        let range = store.attestations_from_height(0);

        assert_eq!(range.len(), 10);
    }

    #[test]
    fn preserves_order() {
        let mut store = AttestationStore::new(9_000);

        for i in 1..=10 {
            let att = Attestation {
                signature: [0; 64],
                data: Vec::new(),
            };
            store.push(i, att);
        }
        let range = store.attestations_from_height(0);

        for (actual, expected) in range.keys().zip(1..=10) {
            assert_eq!(actual, &expected);
        }
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
        let range = store.attestations_from_height(6);

        assert_eq!(range.len(), 5);
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
        let range = store.attestations_from_height(10);

        assert_eq!(range.len(), 1);
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
        let range = store.attestations_from_height(11);

        assert_eq!(range.len(), 0);
    }
}
