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
//!
//! **Nothing here reads a clock, and that is the point.** `core.md` §7's
//! command line makes the table byte-identical across two runs of the same
//! corpus at the same commit, "which is what makes it usable as a gate rather
//! than a report" — so a wall-clock number in it takes the property away, in
//! both formats at once, since `--format json` is the one the harness
//! consumes. This type holds counters and no `Duration`, so `render` cannot
//! vary by inattention; putting the clock back means adding a field, not
//! forgetting one. Where the latency numbers live instead is §7's per-query
//! record — the section says the record "covers ... latency percentiles" and
//! it carries `heuristic_latency_us` per row, which is what `loops.md` §10's
//! *per-stratum* percentiles need and a global p50/p99 could never give.

use std::fmt::Write as _;

use serde::Serialize;
use shared::{Agreement, Strata, Stratum};

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

    /// `match_top1` over the rows that were judged. The number that gets
    /// optimised: a top-1 match cannot be improved by returning more.
    ///
    /// The denominator is the three agreement counters and not `committed`,
    /// because §7 reports coverage on `stratum_prior` and precision on
    /// `stratum_final` — so on a refined query the two live in different rows,
    /// and `committed` is the wrong row's number. The three partition the
    /// judged commits exactly (`Agreement::classify` returns one of them for
    /// every committed row), so where nothing refined this is the old value.
    pub(crate) fn precision(&self) -> f64 {
        ratio(
            self.match_top1,
            self.match_top1 + self.match_contained + self.mismatch,
        )
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
        }
    }

    /// The two halves of a row land in **two** rows when the search refined
    /// its stratum, which is the whole reason `core.md` §7 makes the stratum
    /// two fields: the coverage counters go under `strata.prior()`, so the
    /// denominator is fixed by the reference and does not move when the
    /// implementation changes, and the agreement counters go under
    /// `strata.settled()`, so an answer is judged against the class it turned
    /// out to be.
    pub(crate) fn observe(&mut self, strata: Strata, decision: Decision, agreement: Agreement) {
        if let Some(row) = self.row(strata.prior()) {
            row.queries += 1;
            match decision {
                Decision::Committed => row.committed += 1,
                Decision::Abstained => row.abstained += 1,
                Decision::Failed => row.failed += 1,
            }
        }

        match decision {
            Decision::Committed => {
                let Some(row) = self.row(strata.settled()) else {
                    return;
                };
                match agreement {
                    Agreement::MatchTop1 => row.match_top1 += 1,
                    Agreement::MatchContained => row.match_contained += 1,
                    Agreement::Mismatch { .. } => row.mismatch += 1,
                }
            }
            // There is nothing to judge, so the settled stratum has nothing to
            // say. Written out rather than wildcarded: a fourth decision would
            // have to state which of the two rows it belongs in.
            Decision::Abstained | Decision::Failed => {}
        }
    }

    fn row(&mut self, stratum: Stratum) -> Option<&mut Row> {
        let index = STRATA.iter().position(|known| *known == stratum)?;
        self.rows.get_mut(index)
    }

    pub(crate) fn render(&self, format: Format) -> Result<String, shared::Error> {
        match format {
            Format::Table => Ok(self.as_text()),
            Format::Json => serde_json::to_string_pretty(&Report {
                strata: &self.rows,
                uncollected: self.uncollected,
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

        // A count of corpus rows rather than a measurement of this run, so it
        // is as reproducible as the table it sits under.
        let _ = writeln!(text);
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
