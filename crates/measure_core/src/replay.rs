//! `measure replay`: read `truth.jsonl`, reconstruct the `DocumentSnapshot`
//! and `Query` for each recorded position, run the handler, classify
//! agreement, emit the metric table. No server, no network, no `didOpen` round
//! trips.
//!
//! This is the difference between a tuning iteration costing minutes and
//! costing an afternoon, and it is on the critical path for every language,
//! because a loop whose feedback is slower than its own thinking is bounded by
//! I/O rather than by ideas.
//!
//! **The snapshot and the `Query` are built through `shared`'s constructors**,
//! the same ones the driver uses. That is what makes "the corpus scores the
//! code that ships" structural rather than a matter of discipline: what
//! `measure` measures is the handler, not the driver, so as long as the inputs
//! are built the same way the code under test is genuinely identical.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use shared::proto::DefinitionResult;
use shared::{
    Agreement, Clock, CommitPolicy, DefinitionSite, DocumentUri, DocumentVersion, Error, FileList,
    LanguageHandler, Location, Offset, Outcome, ProjectView, Query, Rope, ServerProfile,
    SnapshotSeed, Strata, Stratum, Trace,
};

use crate::corpus::{Corpus, Repository, verify_checkout};
use crate::record::{self, Decision, Mode, QueryRecord, StratumName};
use crate::table::Table;
use crate::truth::{self, Truth};

pub(crate) struct Replay<'a> {
    pub(crate) handler: &'a dyn LanguageHandler,
    pub(crate) corpus: &'a Corpus,
    pub(crate) server: &'a str,
    pub(crate) clock: &'a dyn Clock,
}

impl Replay<'_> {
    /// One repository's worth of rows, appended to `table` and — when
    /// `--records` was given — to `records`.
    pub(crate) fn run(
        &self,
        repository: &Repository,
        table: &mut Table,
        records: &mut Vec<QueryRecord>,
    ) -> Result<(), Error> {
        let path = self.corpus.truth(self.server, &repository.name);
        let truth = Truth::read(&path)?;
        verify_checkout(repository, Some(&truth.provenance.commit))?;

        // One `FileList` per repository, and a `ProjectView` per query over
        // it: the walk is the expensive part and the scope rules are what the
        // view is for.
        let files = Arc::new(FileList::enumerate(std::slice::from_ref(&repository.path))?);

        // Grouped by file so each document is read and parsed once. The rows
        // arrive in `(file, offset)` order because `enumerate` wrote them that
        // way, so this is a scan rather than a sort.
        let mut current: Option<(Box<str>, shared::DocumentSnapshot)> = None;
        for row in &truth.rows {
            if !row.outcome.is_ground_truth() {
                // `error` and `timeout` are not ground truth. They are excluded
                // from metrics and reported as a coverage figure for the
                // *collection*, which is a quality signal about the
                // repository's build setup rather than about the handler.
                table.uncollected += 1;
                continue;
            }

            if current.as_ref().map(|(file, _)| &**file) != Some(&*row.file) {
                current = self
                    .snapshot(repository, &row.file)?
                    .map(|document| (row.file.clone(), document));
            }
            let Some((_, document)) = &current else {
                continue;
            };

            records.push(self.one(&files, document, row, table)?);
        }

        Ok(())
    }

    fn snapshot(
        &self,
        repository: &Repository,
        file: &str,
    ) -> Result<Option<shared::DocumentSnapshot>, Error> {
        let absolute = repository.path.join(file);
        let Ok(text) = fs::read_to_string(&absolute) else {
            tracing::warn!(path = %absolute.display(), "a recorded file is unreadable");
            return Ok(None);
        };
        let Some(uri) = DocumentUri::from_file_path(&absolute) else {
            return Ok(None);
        };
        let language = self.corpus.language();
        // The same `Deadline::none()` the query below runs under, and for the
        // same reason: a parse abandoned on a slow machine would drop the row
        // entirely, so *coverage* — not just latency — would vary with load
        // (`core.md` §7, "replay enforces no deadline at all").
        let deadline = shared::Deadline::none();
        Ok(Some(
            SnapshotSeed::fresh(
                uri,
                Rope::from(text.as_str()),
                DocumentVersion(0),
                language,
                self.handler.grammar(),
            )
            .realise(&deadline)?,
        ))
    }

    fn one(
        &self,
        files: &Arc<FileList>,
        document: &shared::DocumentSnapshot,
        row: &truth::Row,
        table: &mut Table,
    ) -> Result<QueryRecord, Error> {
        // No deadline at all. A wall-clock deadline would make abstention
        // depend on machine load, so *coverage* — not just latency — would
        // become a property of what else was running.
        let deadline = shared::Deadline::none();
        // DECISION-conformance-012: provisional. The grammar reaches
        // `ProjectView::parse` through the constructor rather than through the
        // method, so it is handed over here.
        let project = ProjectView::new(Arc::clone(files), deadline.clone(), self.handler.grammar());
        let policy = CommitPolicy::permissive();
        // The oracle this truth file was collected against, by the name
        // `servers.toml` gives it — a replay has no child process, so the name
        // is the only identity there is, and it is the same one the provenance
        // header records. A fixture oracle that is not in the matrix resolves
        // to no id, which is the documented "a server we have no profile for"
        // rather than a synthesised one (`core.md` §7).
        let profile = ServerProfile::proxying_named(self.server);

        let query = Query {
            doc: document,
            position: Offset(row.offset),
            project: &project,
            deadline: &deadline,
            server: &profile,
            policy: &policy,
        };

        let started = self.clock.now();
        let answered = self.handler.goto_definition(&query);
        let elapsed = self.clock.now().saturating_duration_since(started);

        let child = serde_json::from_str::<DefinitionResult>(row.answer.get())
            .unwrap_or(DefinitionResult::Null);

        // `answered` is consumed here rather than borrowed: a `Trace` is
        // write-only until it is taken apart, and taking it apart is what
        // `into_parts` does — so the record can only be assembled from an
        // outcome nobody is going to read again.
        let (decision, failure, strata, locations, confidence, parts, extra) = match answered {
            Ok(Outcome::Committed {
                locations,
                confidence,
                strata,
                trace,
            }) => (
                Decision::Committed,
                None,
                strata,
                locations,
                Some(confidence.get()),
                trace.into_parts(),
                None,
            ),
            Ok(Outcome::Abstain {
                reason,
                strata,
                trace,
            }) => (
                Decision::Abstained,
                None,
                strata,
                Vec::new(),
                None,
                trace.into_parts(),
                // The reason goes into `stages` rather than into a column of
                // its own, because `stages` is the field §7 makes the
                // handler's account of what it did and a second reason column
                // would be two vocabularies for one question.
                Some(record::abstain_label(&reason)),
            ),
            // A failure is served as an abstention on the wire and recorded as
            // a failure here, or the per-stratum table cannot tell a hard
            // stratum from a broken handler. There is no outcome and therefore
            // no trace: a handler that returned `Err` reported nothing, and an
            // empty account is the honest record of that.
            Err(error) => (
                Decision::Failed,
                Some(failure_class(&error)),
                Strata::from_reference(Stratum::Unimplemented),
                Vec::new(),
                None,
                Trace::new().into_parts(),
                None,
            ),
        };
        let mut stages = record::stage_labels(parts.stages);
        stages.extend(extra);

        let ours: Vec<DefinitionSite<'_>> = locations.iter().map(DefinitionSite::of).collect();
        let agreement = Agreement::classify(&ours, &child);
        let (agreement_label, severity) = record::agreement_labels(agreement);

        // `elapsed` is not offered to the table: §7's command line makes the
        // table byte-identical across runs, and it goes into the record below
        // instead, which is the one field §7 says a replay does not reproduce.
        table.observe(strata, decision, agreement);

        Ok(QueryRecord {
            uri: document.uri.to_string().into(),
            position: record::position_of(Offset(row.offset)),
            language: self.corpus.language().as_str().into(),
            mode: Mode::Proxy,
            server_health: None,
            decision,
            failure,
            stratum_prior: StratumName(strata.prior()),
            stratum_final: StratumName(strata.settled()),
            confidence,
            margin: parts.margin.map(shared::Margin::get),
            considered: parts.considered.map(|considered| considered.0),
            stages,
            bytes_scanned: parts.bytes_scanned.0,
            files_parsed: record::file_count(parts.files_parsed),
            queued_us: 0,
            stage_us: record::stage_timings(parts.stage_us),
            heuristic_latency_us: micros(elapsed),
            heuristic_locations: locations.iter().map(label).collect(),
            returned: locations.len(),
            truncated_list: false,
            lsp_latency_us: Some(row.latency_us),
            lsp_locations: Some(child_labels(&child)),
            agreement: Some(agreement_label),
            severity,
        })
    }
}

/// The `Error` sub-enum that was converted. Written as an exhaustive match
/// rather than a `Display` string, so a new sub-enum has to be given a name
/// here instead of appearing in the metrics as whatever `thiserror` produced.
fn failure_class(error: &Error) -> Box<str> {
    match error {
        Error::Config(_) => "Config".into(),
        Error::Codec(_) => "Codec".into(),
        Error::Child(_) => "Child".into(),
        Error::Protocol(_) => "Protocol".into(),
        Error::Document(_) => "Document".into(),
        Error::Parse(_) => "Parse".into(),
        Error::Project(_) => "Project".into(),
        Error::Handler(_) => "Handler".into(),
        Error::Encoding(_) => "Encoding".into(),
    }
}

fn label(location: &Location) -> Box<str> {
    record::location_label(location.uri(), location.line())
}

fn child_labels(child: &DefinitionResult) -> Vec<Box<str>> {
    match child {
        DefinitionResult::Null => Vec::new(),
        DefinitionResult::One(location) => {
            vec![record::location_label(
                location.uri(),
                location.range().start.line(),
            )]
        }
        DefinitionResult::Many(locations) => locations
            .iter()
            .map(|location| record::location_label(location.uri(), location.range().start.line()))
            .collect(),
        DefinitionResult::Links(links) => links
            .iter()
            .map(|link| {
                record::location_label(&link.target_uri, link.target_selection_range.start.line())
            })
            .collect(),
    }
}

fn micros(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
}

/// `--records <path>`: the per-query JSONL of §7's record, unchanged and
/// unfiltered. No new schema — a replay row and a field row are the same
/// shape, which is the property the two modes already turn on.
///
/// Digesting these into something readable is the harness's job, not
/// `measure_core`'s: the same split that keeps this crate ignorant of
/// `state/`.
pub(crate) fn write_records(path: &Path, records: &[QueryRecord]) -> Result<(), Error> {
    let mut text = String::new();
    for record in records {
        text.push_str(&serde_json::to_string(record).map_err(|source| {
            shared::CodecError::NotSerializable {
                what: "a query record",
                source,
            }
        })?);
        text.push('\n');
    }
    fs::write(path, text).map_err(|source| shared::ConfigError::ManifestUnreadable {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}
