//! The per-stratum table `replay` prints.
//!
//! `high-level.md` reports coverage and precision **per stratum** and refuses
//! a single rolled-up number, because the mix is not a fact about the tool.
//! This type therefore has no total row, and `core.md` §7's "every metric is
//! per (language, server)" is why a run names one server and never aggregates
//! across them.
//!
//! Exit status is about whether the run happened, not about whether the
//! numbers are good: `replay` exits zero having printed a table full of
//! zeroes. Judging the table is the gate's job, not the measurement's.

use std::fmt::Write as _;
use std::time::Duration;

use serde::Serialize;
use shared::{Agreement, Stratum};

use crate::cli::Format;
use crate::record::{Decision, StratumName};

/// The nine strata, in the order `high-level.md` lists them, so two runs print
/// the same rows in the same order whatever the corpus contained.
const STRATA: [Stratum; 9] = [
    Stratum::LocalBinding,
    Stratum::SameFileModule,
    Stratum::ExplicitImport,
    Stratum::WildcardImport,
    Stratum::AmbiguousName,
    Stratum::ExternalDependency,
    Stratum::MacroGenerated,
    Stratum::TypeInferenceRequired,
    Stratum::Unimplemented,
];

#[derive(Debug, Default)]
pub(crate) struct Table {
    rows: Vec<Row>,
    /// Positions whose truth row was `error` or `timeout`. Reported beside the
    /// table as a quality signal about the corpus, never folded into it.
    pub(crate) uncollected: u64,
    latencies: Vec<u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct Row {
    pub(crate) stratum: Box<str>,
    pub(crate) queries: u64,
    pub(crate) committed: u64,
    pub(crate) abstained: u64,
    pub(crate) failed: u64,
    pub(crate) match_top1: u64,
    pub(crate) match_contained: u64,
    pub(crate) mismatch: u64,
}

impl Row {
    /// Committed over queries. A ratio computed here rather than by every
    /// consumer, because a metric with two definitions is two metrics.
    pub(crate) fn coverage(&self) -> f64 {
        ratio(self.committed, self.queries)
    }

    /// `match_top1` over committed. The number that gets optimised: a top-1
    /// match cannot be improved by returning more.
    pub(crate) fn precision(&self) -> f64 {
        ratio(self.match_top1, self.committed)
    }
}

impl Table {
    pub(crate) fn new() -> Self {
        Self {
            rows: STRATA
                .iter()
                .map(|stratum| Row {
                    stratum: StratumName(*stratum).as_str().into(),
                    ..Row::default()
                })
                .collect(),
            uncollected: 0,
            latencies: Vec::new(),
        }
    }

    pub(crate) fn observe(
        &mut self,
        stratum: Stratum,
        decision: Decision,
        agreement: Agreement,
        elapsed: Duration,
    ) {
        self.latencies
            .push(u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX));

        let Some(index) = STRATA.iter().position(|known| *known == stratum) else {
            return;
        };
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };

        row.queries += 1;
        match decision {
            Decision::Committed => {
                row.committed += 1;
                match agreement {
                    Agreement::MatchTop1 => row.match_top1 += 1,
                    Agreement::MatchContained => row.match_contained += 1,
                    Agreement::Mismatch { .. } => row.mismatch += 1,
                }
            }
            Decision::Abstained => row.abstained += 1,
            Decision::Failed => row.failed += 1,
        }
    }

    pub(crate) fn render(&self, format: Format) -> Result<String, shared::Error> {
        match format {
            Format::Table => Ok(self.as_text()),
            Format::Json => serde_json::to_string_pretty(&Report {
                strata: &self.rows,
                uncollected: self.uncollected,
                heuristic_latency_us: Percentiles::of(&self.latencies),
            })
            .map_err(|source| {
                shared::CodecError::NotSerializable {
                    what: "a replay report",
                    source,
                }
                .into()
            }),
        }
    }

    fn as_text(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "{:<24} {:>8} {:>8} {:>8} {:>8} {:>9} {:>10} {:>9}",
            "stratum", "queries", "commit", "abstain", "fail", "coverage", "precision", "contained"
        );
        for row in &self.rows {
            let _ = writeln!(
                text,
                "{:<24} {:>8} {:>8} {:>8} {:>8} {:>8.1}% {:>9.1}% {:>9}",
                row.stratum,
                row.queries,
                row.committed,
                row.abstained,
                row.failed,
                row.coverage() * 100.0,
                row.precision() * 100.0,
                row.match_contained,
            );
        }

        let percentiles = Percentiles::of(&self.latencies);
        let _ = writeln!(text);
        let _ = writeln!(
            text,
            "heuristic latency: p50 {}us  p99 {}us",
            percentiles.p50, percentiles.p99
        );
        // Reported beside the table rather than in it: `replay` also reports
        // its own wall clock, and what to do about the number is decided when
        // there is one rather than against a target set before a handler and a
        // corpus both existed.
        let _ = writeln!(
            text,
            "positions the oracle never answered: {}",
            self.uncollected
        );
        text
    }
}

#[derive(Debug, Serialize)]
struct Report<'a> {
    strata: &'a [Row],
    uncollected: u64,
    heuristic_latency_us: Percentiles,
}

#[derive(Copy, Clone, Debug, Default, Serialize)]
struct Percentiles {
    p50: u64,
    p99: u64,
}

impl Percentiles {
    fn of(samples: &[u64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            p50: at(&sorted, 50),
            p99: at(&sorted, 99),
        }
    }
}

fn at(sorted: &[u64], percentile: usize) -> u64 {
    // Nearest-rank, which is exact on integers and has no interpolation to
    // disagree about between two implementations of the same number.
    let rank = (sorted.len() * percentile).div_ceil(100).max(1) - 1;
    sorted.get(rank).copied().unwrap_or_default()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a ratio of two counts, for display and for a JSON report; both are bounded by the corpus size and neither is compared against a threshold"
)]
fn ratio(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64
}
