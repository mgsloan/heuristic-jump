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

use shared::proto::DefinitionResult;
use shared::record::{Answered, ChildAnswer, Mode, QueryContext, QueryRecord};
use shared::{
    Agreement, Clock, CommitPolicy, DefinitionSite, DocumentUri, DocumentVersion, Error, FileList,
    LanguageHandler, Offset, ProjectView, Query, Rope, ServerProfile, SnapshotSeed,
};

use crate::corpus::{Corpus, Repository, verify_checkout};
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
        // §7: the header "names exactly one server and version", which is what
        // makes a truth file comparable to itself over time and never silently
        // merged with another's. The commit check above is about the *corpus*
        // having moved; this one is about the file being the wrong file, and
        // the `truth/<server>/` path is not a check — a file copied or hand-
        // moved into it would be replayed under a name it was not collected
        // under, and every metric would be attributed to the wrong oracle.
        for (field, recorded, found) in [
            ("server", &truth.provenance.server, self.server),
            (
                "language",
                &truth.provenance.language,
                self.corpus.language().as_str(),
            ),
        ] {
            if &**recorded != found {
                return Err(shared::ConfigError::ProvenanceDrift {
                    path,
                    field,
                    recorded: recorded.clone(),
                    found: found.into(),
                }
                .into());
            }
        }

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
        // `conformance-012` (answered). The grammar reaches
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

        // `answered` is consumed rather than borrowed, and the classification
        // is `shared`'s rather than this crate's: the shim emits the same
        // record from the same three endings, and a second copy of this match
        // is exactly where "a replay row is byte comparable with a field row"
        // would quietly stop being true (`core.md` §7).
        let answered = Answered::of(answered);
        let (decision, strata) = (answered.decision, answered.strata);

        let ours: Vec<DefinitionSite<'_>> =
            answered.locations.iter().map(DefinitionSite::of).collect();
        let agreement = Agreement::classify(&ours, &child);

        // `elapsed` is not offered to the table: §7's command line makes the
        // table byte-identical across runs, and it goes into the record below
        // instead, which is the one field §7 says a replay does not reproduce.
        table.observe(strata, decision, agreement);

        let mut record = QueryRecord::new(
            &QueryContext {
                uri: &document.uri,
                position: Offset(row.offset),
                language: self.corpus.language(),
                mode: Mode::Proxy,
                server_health: None,
                // Replay has no queue. The field is §5's, and §5's deadline is
                // the one thing a replay does not enforce.
                queued: shared::Micros(0),
                elapsed: shared::record::micros(elapsed),
            },
            answered,
        );
        // Frozen rather than raced, which is what makes replay's oracle half
        // present at all: the truth file already holds the latency and the
        // answer, so there is no second round trip to wait for.
        record.answered_by(ChildAnswer {
            latency: shared::Micros(row.latency_us),
            locations: shared::record::definition_labels(&child),
            agreement: Some(agreement),
        });
        Ok(record)
    }
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
