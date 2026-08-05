//! `shim.md` §13's `report/trace.rs`: `--trace=<path>`, and the rule that
//! decides *when* a row is written.
//!
//! `core.md` §7 says each query emits one JSONL record "once both answers are
//! known (or the query is resolved as abstained)", and that sentence is the
//! whole of this module. A proxied row is incomplete for as long as the two
//! answers are apart — which is exactly as long as the `PendingQuery` lives,
//! and for the same reason. Standalone has no second answer coming, so its
//! rows are complete when the handler returns ([`Traces::finished`]).
//!
//! **Which answer arrives first is not this module's business, and it used to
//! be.** An earlier revision said the shim answers first and the child later,
//! which was a property of dispatch happening in line on `core`'s thread
//! rather than a fact about either party: the child is a whole process away,
//! but a handler that reads candidate files is not fast. Now that a query is
//! answered on `shim.md` §10's pool, `Actor` holds the child's answer until
//! the worker comes home ([`crate::Actor::finished`]), so a row still reaches
//! [`Traces::awaiting_child`] before [`Traces::child_answered`] — the order
//! here is the same, and it is `core`'s to preserve rather than the wire's to
//! promise.
//!
//! **The record type is `shared`'s and the writer is here.** §9's graph gives
//! `driver` no edge to `measure_core`, and §7 requires the shim's row and the
//! replay's row to be the same shape, so the shape lives where both can reach
//! it (`shared::record`) and only the file handle is local.
//!
//! Two things it deliberately does not do. It does not aggregate: digesting
//! records into something readable is the harness's job, the same split that
//! keeps `measure_core` ignorant of `state/`. And it does not fail a query — a
//! write that fails is logged and the sink is dropped, because observability
//! that can take the shim down is worse than observability that stops.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use shared::record::{ChildAnswer, QueryRecord};
use shared::{ConfigError, EditorRequestId, Error, Map};

/// Where §7's records go, or that they are not being written.
///
/// An enum rather than an `Option<PathBuf>` so that `Config::new(mode,
/// deadline, None)` is unspellable and the default reads as a decision at
/// every call site (`CLAUDE.md`: no bare `Option` parameters).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Tracing {
    Off,
    To(PathBuf),
}

/// The sink and the rows waiting for their oracle.
///
/// The waiting rows are keyed by `EditorRequestId` and are the trace half of
/// `PendingQueries`: an id is put in by [`Traces::awaiting_child`] and taken
/// out by exactly one of [`Traces::child_answered`] and [`Traces::dropped`],
/// so a query cannot emit two rows and a cancelled one cannot emit a row that
/// claims an oracle it never heard from.
///
/// They are two maps rather than one field on `PendingQuery` because they have
/// different lifetimes in the one case that matters: a query the shim did not
/// answer is still pending — the child's response has to be matched to it —
/// and has no row, since there is no heuristic answer to describe.
#[derive(Debug)]
pub struct Traces {
    sink: Option<Sink>,
    outstanding: Map<EditorRequestId, QueryRecord>,
}

#[derive(Debug)]
struct Sink {
    path: PathBuf,
    file: BufWriter<File>,
}

impl Traces {
    /// `Tracing::Off`: nothing is written and nothing is held. The held rows
    /// are the cost of the flag, so a shim nobody asked to trace does not pay
    /// a map entry per outstanding query.
    pub fn off() -> Self {
        Self {
            sink: None,
            outstanding: Map::default(),
        }
    }

    /// Appending rather than truncating, because a corpus run is many
    /// invocations against one path and a shim that truncated would leave the
    /// records of whichever process exited last.
    pub fn to_path(path: &Path) -> Result<Self, Error> {
        let file = File::options()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|source| ConfigError::TraceUnwritable {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(Self {
            sink: Some(Sink {
                path: path.to_path_buf(),
                file: BufWriter::new(file),
            }),
            outstanding: Map::default(),
        })
    }

    pub fn resolve(tracing: &Tracing) -> Result<Self, Error> {
        match tracing {
            Tracing::Off => Ok(Self::off()),
            Tracing::To(path) => Self::to_path(path),
        }
    }

    /// Standalone's whole story: there is no second answer to wait for, so the
    /// row is complete and `lsp_latency_us`, `lsp_locations`, `agreement` and
    /// `severity` stay `null` — which is what §7 says they are in that mode.
    pub fn finished(&mut self, record: QueryRecord) {
        self.write(&record);
    }

    /// Proxying: the row exists and is not writable until the child replies.
    pub fn awaiting_child(&mut self, editor_id: EditorRequestId, record: QueryRecord) {
        if self.sink.is_none() {
            return;
        }
        if self.outstanding.insert(editor_id, record).is_some() {
            // The same id twice while the first was outstanding. `PendingQueries`
            // logs it too and keeps the newer record; the older row is lost
            // rather than written half-complete, since the answer it describes
            // was superseded before any oracle arrived.
            tracing::warn!("a second trace row arrived under an id already outstanding");
        }
    }

    /// §7's "once both answers are known". Nothing to do when the shim did not
    /// answer this query — there is no row, because there is no heuristic
    /// answer to describe.
    pub fn child_answered(&mut self, editor_id: &EditorRequestId, child: ChildAnswer) {
        let Some(mut record) = self.outstanding.remove(editor_id) else {
            return;
        };
        record.answered_by(child);
        self.write(&record);
    }

    /// `$/cancelRequest`, and the child response that never came. The row is
    /// discarded rather than written with a null oracle half: a cancelled
    /// query has no `agreement` because nobody was ever going to answer it,
    /// and a row that looked like standalone's would put a proxied query in
    /// the bucket §7 keeps for queries with no second answer at all.
    pub fn dropped(&mut self, editor_id: &EditorRequestId) {
        if self.outstanding.remove(editor_id).is_some() {
            tracing::debug!("a traced query was cancelled before its oracle answered");
        }
    }

    pub fn outstanding(&self) -> usize {
        self.outstanding.len()
    }

    /// One line, flushed. Flushing per record rather than per buffer because
    /// the shim is long-lived and a trace nobody can read until the editor
    /// quits is not an observation anyone can act on; the buffer is still
    /// worth having, since a record is several small writes.
    fn write(&mut self, record: &QueryRecord) {
        let Some(sink) = &mut self.sink else {
            return;
        };
        let line = match serde_json::to_string(record) {
            Ok(line) => line,
            Err(source) => {
                tracing::error!(%source, "a query record would not serialize");
                return;
            }
        };
        if let Err(source) = writeln!(sink.file, "{line}").and_then(|()| sink.file.flush()) {
            // Dropped rather than retried: the commonest cause is a full or
            // read-only filesystem, and a shim that retried per query would
            // spend the deadline it exists to protect on an error nobody is
            // reading.
            tracing::error!(
                path = %sink.path.display(),
                %source,
                "the trace could not be written; no further records will be",
            );
            self.sink = None;
            self.outstanding.clear();
        }
    }
}
