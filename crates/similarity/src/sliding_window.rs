//! An occurrence window that moves without rebuilding, and the weighted
//! overlap over it.
//!
//! Ported whole from the prior implementation. Nothing in `resolution.md` §5's
//! pipeline slides a window — go-to-definition asks one question at one
//! position — so this is the part of the toolkit most clearly present for
//! later rather than now. It is what an incremental scorer would need if one
//! is ever wanted, and it is the only consumer of `Occurrences::remove_hash`.

use std::collections::VecDeque;
use std::fmt::Debug;

use crate::occurrences::{HashFrom, Occurrences, ratio};

#[derive(Debug)]
pub struct SlidingWindow<D, T, S> {
    target: T,
    intersection: Occurrences<S>,
    regions: VecDeque<WeightedOverlapRegion<D, S>>,
    numerator: u32,
    window_count: u32,
    jaccard_denominator_part: u32,
}

#[derive(Debug)]
struct WeightedOverlapRegion<D, S> {
    data: D,
    added_hashes: Vec<AddedHash<S>>,
    window_count_delta: u32,
}

#[derive(Debug)]
struct AddedHash<S> {
    hash: HashFrom<S>,
    target_count: u32,
}

impl<D, T: AsRef<Occurrences<S>>, S> SlidingWindow<D, T, S> {
    pub fn new(target: T) -> Self {
        Self::with_capacity(target, 0)
    }

    pub fn with_capacity(target: T, capacity: usize) -> Self {
        let jaccard_denominator_part = target.as_ref().len();
        Self {
            target,
            intersection: Occurrences::default(),
            regions: VecDeque::with_capacity(capacity),
            numerator: 0,
            window_count: 0,
            jaccard_denominator_part,
        }
    }

    pub fn clear(&mut self) {
        self.intersection.clear();
        self.regions.clear();
        self.numerator = 0;
        self.window_count = 0;
        self.jaccard_denominator_part = self.target.as_ref().len();
    }

    pub fn push_back(&mut self, data: D, hashes: impl IntoIterator<Item = HashFrom<S>>) {
        let mut added_hashes = Vec::new();
        let mut window_count_delta = 0;
        for hash in hashes {
            window_count_delta += 1;
            let target_count = self.target.as_ref().get_count(hash);
            if target_count > 0 {
                added_hashes.push(AddedHash { hash, target_count });
                let window_hash_count = self.intersection.add_hash(hash);
                if window_hash_count <= target_count {
                    self.numerator += 1;
                } else {
                    self.jaccard_denominator_part += 1;
                }
            }
        }
        self.window_count += window_count_delta;
        self.regions.push_back(WeightedOverlapRegion {
            data,
            added_hashes,
            window_count_delta,
        });
    }

    /// `None` when there is no region left to retire. The original panicked
    /// here; nothing consumed it yet, so the emptiness is pushed to the caller
    /// rather than asserted (`CLAUDE.md`).
    pub fn pop_front(&mut self) -> Option<D> {
        let removed = self.regions.pop_front()?;

        for AddedHash { hash, target_count } in removed.added_hashes {
            let window_hash_count = self.intersection.remove_hash(hash);
            if window_hash_count < target_count {
                if let Some(numerator) = self.numerator.checked_sub(1) {
                    self.numerator = numerator;
                } else {
                    debug_assert!(false, "underflow in sliding window text similarity");
                }
            } else if let Some(jaccard_denominator_part) =
                self.jaccard_denominator_part.checked_sub(1)
            {
                self.jaccard_denominator_part = jaccard_denominator_part;
            } else {
                debug_assert!(false, "underflow in sliding window text similarity");
            }
        }

        if let Some(window_count) = self.window_count.checked_sub(removed.window_count_delta) {
            self.window_count = window_count;
        } else {
            debug_assert!(false, "underflow in sliding window text similarity");
        }

        Some(removed.data)
    }

    pub fn weighted_overlap_coefficient(&self) -> f32 {
        let denominator = self.target.as_ref().len().min(self.window_count);
        ratio(self.numerator as usize, denominator as usize)
    }

    pub fn weighted_jaccard_similarity(&self) -> f32 {
        let mut denominator = self.jaccard_denominator_part;
        if let Some(other_denominator_part) = self.window_count.checked_sub(self.intersection.len())
        {
            denominator += other_denominator_part;
        } else {
            debug_assert!(false, "underflow in sliding window text similarity");
        }
        ratio(self.numerator as usize, denominator as usize)
    }
}
