//! What upstream's `#[cfg(test)]` modules reached outside the crate for.
//! `design/rope-modifications.md` section 4 has the table and section 7 the
//! argument for keeping the tests at all.
//!
//! Also `#[path]`-included by `benches/rope_benchmark.rs`, which is the sixth
//! `util` import site and the reason this is a file rather than an inline
//! module: a bench target cannot see a `#[cfg(test)]` module. Each of the two
//! consumers uses a subset, so `allow(dead_code)` rather than `expect` —
//! `expect` would fire in whichever target did not use the item.
#![allow(dead_code)]

use rand::prelude::*;
use std::env;

/// Replaces `#[gpui::test(iterations = N)]`, which for rope's randomised
/// tests does exactly one job: run the body N times with deterministic seeds
/// and say which seed failed. Nothing about gpui is involved -- they take
/// `mut rng: StdRng` and nothing else, no `TestAppContext` and no async.
///
/// The `SEED` and `ITERATIONS` overrides behave as gpui's do
/// (`crates/gpui/src/test.rs`, `calculate_seeds`), so a failing seed is rerun
/// the same way it is upstream.
pub fn seeded(iterations: u64, test: fn(StdRng)) {
    let iterations: u64 = env::var("ITERATIONS")
        .map(|var| var.parse().expect("invalid `ITERATIONS` variable"))
        .unwrap_or(iterations);
    let first_seed: u64 = env::var("SEED")
        .map(|var| var.parse().expect("invalid `SEED` variable"))
        .unwrap_or(0);

    for seed in first_seed..first_seed + iterations {
        // Printed before the run rather than after a failure, so the seed is
        // on stdout whichever way the body ends.
        eprintln!("seed = {seed}");
        test(StdRng::seed_from_u64(seed));
    }
}

/// Lifted verbatim from Zed's `crates/util/src/util.rs` (Apache-2.0), where it
/// lives in a `#[cfg(any(test, feature = "test-support"))] mod rng`.
/// `design/rope-modifications.md` section 4.
pub struct RandomCharIter<T: Rng> {
    rng: T,
    simple_text: bool,
}

impl<T: Rng> RandomCharIter<T> {
    pub fn new(rng: T) -> Self {
        Self {
            rng,
            simple_text: std::env::var("SIMPLE_TEXT").is_ok_and(|v| !v.is_empty()),
        }
    }

    pub fn with_simple_text(mut self) -> Self {
        self.simple_text = true;
        self
    }
}

impl<T: Rng> Iterator for RandomCharIter<T> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if self.simple_text {
            return if self.rng.random_range(0..100) < 5 {
                Some('\n')
            } else {
                Some(self.rng.random_range(b'a'..b'z' + 1).into())
            };
        }

        match self.rng.random_range(0..100) {
            // whitespace
            0..=19 => [' ', '\n', '\r', '\t'].choose(&mut self.rng).copied(),
            // two-byte greek letters
            20..=32 => char::from_u32(self.rng.random_range(('α' as u32)..('ω' as u32 + 1))),
            // // three-byte characters
            33..=45 => ['✋', '✅', '❌', '❎', '⭐']
                .choose(&mut self.rng)
                .copied(),
            // // four-byte characters
            46..=58 => ['🍐', '🏀', '🍗', '🎉'].choose(&mut self.rng).copied(),
            // ascii letters
            _ => Some(self.rng.random_range(b'a'..b'z' + 1).into()),
        }
    }
}
