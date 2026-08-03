//! `truth.jsonl`: the proper LSP's answer for a given position at a given
//! commit, which is a *fact about the corpus* rather than about our code, so
//! it is collected once and frozen.
//!
//! A truth file is regenerated, never edited. Metrics compared across two
//! corpus versions are not comparable, and a partially refreshed corpus is the
//! worst case: it looks like a regression.
//!
//! **The answer is stored as the raw JSON the server sent.** Not as a
//! projection written back out — `core.md` §8.2 forbids round-tripping a wire
//! type, and `DefinitionResult` is a read projection with no `Serialize` for
//! exactly that reason. Keeping the bytes means replay deserializes the
//! oracle's answer with the same code the shim reads a live one with, which is
//! the property §6 needs and a re-serialized copy would quietly weaken.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use shared::{CodecError, ConfigError, Error};

/// The provenance header, written as the file's first line.
///
/// It names exactly one server and version, which is what makes a truth file
/// comparable to itself over time and never silently merged with another's.
///
/// Public because [`check_resumable`] is, and that is public because the
/// property it holds — a resume refuses a header this run would not have
/// written — is one a test has to be able to drive with a header of its own.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub repository: Box<str>,
    /// Replay refuses to run against a truth file whose repository commit does
    /// not match the checkout, rather than silently reporting metrics for
    /// positions that have since moved.
    pub commit: Box<str>,
    pub language: Box<str>,
    pub server: Box<str>,
    pub server_version: Box<str>,
    /// The locked revision of the grammar the positions were enumerated with,
    /// from `corpus::locked_grammar`. A literal here is the failure mode: two
    /// collections under different grammar pins would produce identical
    /// headers.
    pub grammar: Box<str>,
    pub measure_version: Box<str>,
    /// A partially collected truth file is marked incomplete and is never
    /// consumed by replay.
    pub complete: bool,
}

impl Provenance {
    /// Every field but `complete`, in declaration order.
    ///
    /// A resume appends rows to a file whose header is already written, so the
    /// only honest condition is that the header this run *would* write is the
    /// header already there — not just its server version, which was the whole
    /// of the check while a resume was free to write the current `HEAD` over
    /// rows collected at an older one.
    ///
    /// Written as a full destructuring rather than a list of comparisons, so a
    /// new provenance field is a compile error here rather than a field nothing
    /// checks. `complete` is the one exclusion and is named as one: a resume
    /// exists precisely because the file on disk is incomplete.
    pub(crate) fn drift(&self, wanted: &Self) -> Option<Drift> {
        let Self {
            repository,
            commit,
            language,
            server,
            server_version,
            grammar,
            measure_version,
            complete: _,
        } = self;

        [
            ("repository", repository, &wanted.repository),
            ("commit", commit, &wanted.commit),
            ("language", language, &wanted.language),
            ("server", server, &wanted.server),
            ("server_version", server_version, &wanted.server_version),
            ("grammar", grammar, &wanted.grammar),
            ("measure_version", measure_version, &wanted.measure_version),
        ]
        .into_iter()
        .find(|(_, recorded, found)| recorded != found)
        .map(|(field, recorded, found)| Drift {
            field,
            recorded: recorded.clone(),
            found: found.clone(),
        })
    }
}

/// One field of a header that does not agree with the run reading it.
#[derive(Debug)]
pub(crate) struct Drift {
    pub(crate) field: &'static str,
    pub(crate) recorded: Box<str>,
    pub(crate) found: Box<str>,
}

impl Drift {
    pub(crate) fn at(self, path: &Path) -> Error {
        ConfigError::ProvenanceDrift {
            path: path.to_path_buf(),
            field: self.field,
            recorded: self.recorded,
            found: self.found,
        }
        .into()
    }
}

/// `collect` resumes by default, and this is what makes resuming safe: the
/// partial file on disk must carry the header this run would have written.
///
/// Public for the reason `replay_table` is — a property nothing can call is a
/// property nothing can assert, and `collect` itself cannot be driven from a
/// test without a language server. Nothing on disk is `Ok(())`: there is no
/// resume, so there is nothing to disagree with.
pub fn check_resumable(path: &Path, wanted: &Provenance) -> Result<(), Error> {
    match Truth::read_partial(path)? {
        Some(existing) => match existing.provenance.drift(wanted) {
            Some(drift) => Err(drift.at(path)),
            None => Ok(()),
        },
        None => Ok(()),
    }
}

/// `data-collection.md` §4's four outcomes, kept distinct. Collapsing `Error`
/// or `Timeout` into `None` is the mistake that quietly inflates precision
/// later, because the heuristic gets credit for abstaining where the oracle
/// merely failed.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Resolved,
    None,
    Error,
    Timeout,
}

impl Outcome {
    /// Only `Resolved` and `None` are ground truth. The other two are excluded
    /// from metrics and reported as a coverage figure for the *collection*,
    /// which is itself a quality signal about the repository's build setup.
    pub(crate) fn is_ground_truth(self) -> bool {
        match self {
            Outcome::Resolved | Outcome::None => true,
            Outcome::Error | Outcome::Timeout => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Row {
    /// Repository-relative, so a truth file survives the corpus root moving.
    pub(crate) file: Box<str>,
    pub(crate) offset: usize,
    pub(crate) outcome: Outcome,
    /// The `result` field of the server's response, verbatim. `null` for every
    /// outcome but `Resolved`.
    pub(crate) answer: Box<RawValue>,
    /// Recorded on `error` and `timeout` rows too — for `timeout` it is the
    /// cap rather than an observation, and `outcome` is what says so. Dropping
    /// the timing on those would bias the distribution toward the fast tail
    /// exactly where the tool is most valuable.
    pub(crate) latency_us: u64,
}

/// A truth file, read whole. Small enough to: 20k rows of a few hundred bytes.
#[derive(Debug)]
pub(crate) struct Truth {
    pub(crate) provenance: Provenance,
    pub(crate) rows: Vec<Row>,
}

impl Truth {
    pub(crate) fn read(path: &Path) -> Result<Self, Error> {
        let (provenance, rows) = read_lines(path)?;
        if !provenance.complete {
            return Err(ConfigError::ArtifactIncomplete {
                path: path.to_path_buf(),
            }
            .into());
        }
        Ok(Self { provenance, rows })
    }

    /// What `collect --restart` refuses to look at and a resume needs: the
    /// header and rows of a file that may still be incomplete.
    pub(crate) fn read_partial(path: &Path) -> Result<Option<Self>, Error> {
        if !path.exists() {
            return Ok(None);
        }
        let (provenance, rows) = read_lines(path)?;
        Ok(Some(Self { provenance, rows }))
    }
}

fn read_lines(path: &Path) -> Result<(Provenance, Vec<Row>), Error> {
    let file = File::open(path).map_err(|source| ConfigError::ManifestUnreadable {
        path: path.to_path_buf(),
        source,
    })?;

    let mut lines = BufReader::new(file).lines().enumerate();
    let Some((_, first)) = lines.next() else {
        return Err(ConfigError::ArtifactMissing {
            path: path.to_path_buf(),
        }
        .into());
    };
    let first = first.map_err(|source| ConfigError::ManifestUnreadable {
        path: path.to_path_buf(),
        source,
    })?;
    let provenance = serde_json::from_str::<Provenance>(&first).map_err(|source| {
        ConfigError::ArtifactMalformed {
            path: path.to_path_buf(),
            line: 1,
            source,
        }
    })?;

    let mut rows = Vec::new();
    for (index, line) in lines {
        let line = line.map_err(|source| ConfigError::ManifestUnreadable {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        rows.push(serde_json::from_str::<Row>(&line).map_err(|source| {
            ConfigError::ArtifactMalformed {
                path: path.to_path_buf(),
                line: index + 1,
                source,
            }
        })?);
    }

    Ok((provenance, rows))
}

/// Append-only, with the header rewritten when the run finishes.
///
/// Checkpointing is what makes a hundred machine-hours survivable: the file on
/// disk is always a valid prefix, and its header says `complete: false` until
/// the last row is in.
#[derive(Debug)]
pub(crate) struct Writer {
    path: PathBuf,
    file: File,
    provenance: Provenance,
}

impl Writer {
    pub(crate) fn create(path: &Path, provenance: Provenance) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::ManifestUnreadable {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let file = File::create(path).map_err(|source| ConfigError::ManifestUnreadable {
            path: path.to_path_buf(),
            source,
        })?;
        let mut writer = Self {
            path: path.to_path_buf(),
            file,
            provenance,
        };
        let header = encode(&writer.provenance, "a truth file's provenance header")?;
        writer.write_line(&header)?;
        Ok(writer)
    }

    pub(crate) fn append(&mut self, row: &Row) -> Result<(), Error> {
        let line = encode(row, "a truth row")?;
        self.write_line(&line)
    }

    /// Rewrites the whole file with `complete: true`. A rewrite rather than an
    /// in-place header patch because the header's length changes, and a
    /// truncated header is a file nothing can read at all.
    pub(crate) fn finish(self, rows: &[Row]) -> Result<(), Error> {
        let mut provenance = self.provenance;
        provenance.complete = true;

        let mut text = encode(&provenance, "a truth file's provenance header")?;
        text.push('\n');
        for row in rows {
            text.push_str(&encode(row, "a truth row")?);
            text.push('\n');
        }
        fs::write(&self.path, text).map_err(|source| ConfigError::ManifestUnreadable {
            path: self.path,
            source,
        })?;
        Ok(())
    }

    fn write_line(&mut self, line: &str) -> Result<(), Error> {
        writeln!(self.file, "{line}").map_err(|source| ConfigError::ManifestUnreadable {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

/// The answer stored for every outcome but `Resolved`.
pub(crate) fn null_answer() -> Box<RawValue> {
    RawValue::NULL.to_owned()
}

fn encode<T: Serialize>(value: &T, what: &'static str) -> Result<String, Error> {
    serde_json::to_string(value)
        .map_err(|source| CodecError::NotSerializable { what, source }.into())
}
