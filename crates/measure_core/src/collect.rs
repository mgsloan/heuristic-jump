//! `measure collect`: spawn the server, wait for it, drive `didOpen` across
//! the repository, ask the LSP, write `truth.jsonl`.
//!
//! Slow, run rarely, and its output is a frozen artifact in the corpus root
//! and never in the repository. The proper LSP's answer for a given position
//! at a given commit is a *fact about the corpus*, not about our code.

use std::fs;
use std::path::Path;
use std::time::Duration;

use shared::proto::{
    ClientDefinitionCapabilities, ClientDefinitionParams, ClientDidCloseParams,
    ClientDidOpenParams, ClientGeneralCapabilities, ClientIdentity, ClientInitializeParams,
    ClientOfferedCapabilities, ClientTextDocumentCapabilities, ClientTextDocumentIdentifier,
    ClientTextDocumentItem, DefinitionResult, InitializeResult, WirePosition,
};
use shared::{ChildError, Clock, DocumentUri, DocumentVersion, Error, Offset, Rope};

use crate::client::{Client, OFFERED_ENCODINGS, RawResult, settled_encoding};
use crate::corpus::{Corpus, Repository, ServerEntry, grammar_pin, verify_checkout};
use crate::positions::{self, Position};
use crate::truth::{self, Outcome, Provenance, Row, Truth};

/// How many positions between rewrites of the file on disk. A crash costs
/// minutes rather than the repository.
const CHECKPOINT_EVERY: usize = 200;

/// `data-collection.md` §4: after readiness is signalled, issue a small set of
/// known-answerable probe queries and require them to resolve before starting
/// the run. A server that claims ready and answers nothing is a condition to
/// detect at position zero, not at position 20,000.
const PROBES: usize = 32;

pub(crate) struct Collection<'a> {
    pub(crate) corpus: &'a Corpus,
    pub(crate) server: &'a ServerEntry,
    pub(crate) clock: &'a dyn Clock,
    pub(crate) restart: bool,
}

impl Collection<'_> {
    pub(crate) fn run(&self, repository: &Repository) -> Result<(), Error> {
        let commit = verify_checkout(repository, None)?;
        let path = self.corpus.truth(&self.server.name, &repository.name);

        // Built before anything is read, because it is also the *condition* on
        // resuming: `data-collection.md` §4 says a fresh collection against a
        // different server version warns and continues, since the version in
        // the header is what makes the file interpretable, while a resume
        // refuses — half a file from one version and half from another is the
        // one outcome with no honest provenance header. The same argument is
        // what makes the commit part of the check and not just the version: a
        // resume that wrote the current `HEAD` over rows collected at an older
        // one produces a file whose header is true of none of it.
        let provenance = Provenance {
            repository: repository.name.clone(),
            commit,
            language: self.corpus.language().as_str().into(),
            server: self.server.name.clone(),
            server_version: self.server.version.clone(),
            grammar: grammar_pin(self.corpus.language())?,
            measure_version: env!("CARGO_PKG_VERSION").into(),
            complete: false,
        };

        let existing = if self.restart {
            if let Err(error) = remove_partial(&path) {
                tracing::warn!(%error, path = %path.display(), "discarding the partial truth file");
            }
            None
        } else {
            Truth::read_partial(&path)?
        };
        if let Some(existing) = &existing
            && let Some(drift) = existing.provenance.drift(&provenance)
        {
            return Err(drift.at(&path));
        }

        let all = positions::read(&self.corpus.positions(&repository.name))?;
        let done = existing.as_ref().map_or(0, |existing| existing.rows.len());
        let mut rows = existing.map(|existing| existing.rows).unwrap_or_default();

        if done >= all.len() {
            tracing::info!(repository = %repository.name, "already collected");
            return Ok(());
        }

        let mut client = Client::start(&self.server.command)?;
        let encoding = self.initialize(&mut client, repository)?;
        let mut writer = truth::Writer::create(&path, provenance)?;
        for row in &rows {
            writer.append(row)?;
        }

        // Probes first, on positions that are about to be asked anyway: a
        // resolved answer is the evidence the index is built, and asking for
        // it costs nothing that the run was not going to spend.
        self.wait_until_useful(&mut client, repository, encoding, &all)?;

        let mut open: Option<Box<str>> = None;
        for (index, position) in all.iter().enumerate().skip(done) {
            if open.as_deref() != Some(&*position.file) {
                if let Some(previous) = &open {
                    self.close(&mut client, repository, previous)?;
                }
                self.open(&mut client, repository, &position.file)?;
                open = Some(position.file.clone());
            }
            rows.push(self.ask(&mut client, repository, encoding, position)?);

            if rows.len() % CHECKPOINT_EVERY == 0
                && let Some(row) = rows.last()
            {
                writer.append(row)?;
                tracing::info!(
                    repository = %repository.name,
                    done = index + 1,
                    total = all.len(),
                    "checkpoint"
                );
            }
        }

        client.stop(self.clock);
        writer.finish(&rows)
    }

    fn initialize(
        &self,
        client: &mut Client,
        repository: &Repository,
    ) -> Result<shared::proto::PositionEncoding, Error> {
        let Some(root) = DocumentUri::from_file_path(&repository.path) else {
            return Err(shared::ConfigError::RepositoryMissing {
                path: repository.path.clone(),
            }
            .into());
        };

        let (result, _) = client.request::<_, InitializeResult>(
            "initialize",
            ClientInitializeParams {
                process_id: Some(std::process::id()),
                root_uri: root.clone(),
                workspace_folders: vec![shared::proto::ClientWorkspaceFolder {
                    uri: root,
                    name: repository.name.clone(),
                }],
                capabilities: ClientOfferedCapabilities {
                    general: ClientGeneralCapabilities {
                        position_encodings: OFFERED_ENCODINGS.to_vec(),
                    },
                    text_document: ClientTextDocumentCapabilities {
                        definition: ClientDefinitionCapabilities { link_support: true },
                    },
                },
                client_info: ClientIdentity {
                    name: "measure",
                    version: env!("CARGO_PKG_VERSION"),
                },
            },
            self.clock,
        )?;

        client.notify("initialized", EmptyParams {})?;
        Ok(settled_encoding(
            result.and_then(|result| result.capabilities.position_encoding),
        ))
    }

    /// Wait by *asking*, which is the only readiness signal every server has
    /// in common. `shim.md` §6's adapters interpret progress notifications to
    /// race a server; here the same knowledge would only be used to wait, and
    /// a resolved probe is stronger evidence than any notification.
    fn wait_until_useful(
        &self,
        client: &mut Client,
        repository: &Repository,
        encoding: shared::proto::PositionEncoding,
        all: &[Position],
    ) -> Result<(), Error> {
        let mut open: Option<Box<str>> = None;
        for position in all.iter().take(PROBES) {
            if open.as_deref() != Some(&*position.file) {
                if let Some(previous) = &open {
                    self.close(client, repository, previous)?;
                }
                self.open(client, repository, &position.file)?;
                open = Some(position.file.clone());
            }
            let row = self.ask(client, repository, encoding, position)?;
            if row.outcome == Outcome::Resolved {
                if let Some(previous) = &open {
                    self.close(client, repository, previous)?;
                }
                return Ok(());
            }
        }

        Err(ChildError::NeverReady {
            command: self
                .server
                .command
                .first()
                .map(Into::into)
                .unwrap_or_default(),
        }
        .into())
    }

    fn open(&self, client: &mut Client, repository: &Repository, file: &str) -> Result<(), Error> {
        let absolute = repository.path.join(file);
        let text = fs::read_to_string(&absolute).map_err(|source| shared::ProjectError::Read {
            path: absolute.clone(),
            source,
        })?;
        let Some(uri) = DocumentUri::from_file_path(&absolute) else {
            return Err(shared::ConfigError::RepositoryMissing { path: absolute }.into());
        };

        client.notify(
            "textDocument/didOpen",
            ClientDidOpenParams {
                text_document: ClientTextDocumentItem {
                    uri,
                    language_id: self.corpus.language(),
                    version: DocumentVersion(1),
                    text: text.into(),
                },
            },
        )
    }

    fn close(&self, client: &mut Client, repository: &Repository, file: &str) -> Result<(), Error> {
        let absolute = repository.path.join(file);
        let Some(uri) = DocumentUri::from_file_path(&absolute) else {
            return Ok(());
        };
        client.notify(
            "textDocument/didClose",
            ClientDidCloseParams {
                text_document: ClientTextDocumentIdentifier { uri },
            },
        )
    }

    fn ask(
        &self,
        client: &mut Client,
        repository: &Repository,
        encoding: shared::proto::PositionEncoding,
        position: &Position,
    ) -> Result<Row, Error> {
        let absolute = repository.path.join(&*position.file);
        let text = fs::read_to_string(&absolute).map_err(|source| shared::ProjectError::Read {
            path: absolute.clone(),
            source,
        })?;
        let Some(uri) = DocumentUri::from_file_path(&absolute) else {
            return Err(shared::ConfigError::RepositoryMissing { path: absolute }.into());
        };
        let wire = WirePosition::encode(
            Offset(position.offset),
            encoding,
            &Rope::from(text.as_str()),
        )?;

        let asked = client.request::<_, RawResult>(
            "textDocument/definition",
            ClientDefinitionParams {
                text_document: ClientTextDocumentIdentifier { uri },
                position: wire,
            },
            self.clock,
        );

        let (outcome, answer, latency) = match asked {
            Ok((answer, latency)) => classify(answer, latency),
            // An error response is recorded rather than dropped: collapsing it
            // into `none` would give the heuristic credit for abstaining where
            // the oracle merely failed.
            Err(error) => {
                tracing::debug!(%error, file = %position.file, offset = position.offset, "the server errored");
                (Outcome::Error, truth::null_answer(), Duration::ZERO)
            }
        };

        Ok(Row {
            file: position.file.clone(),
            offset: position.offset,
            outcome,
            answer,
            latency_us: micros(latency),
        })
    }
}

/// `--restart` is the destructive option and therefore the explicit one, so
/// what it destroys is reported rather than dropped. The effect would be right
/// either way — `Writer::create` truncates — but a fallible operation on the
/// destructive path that says nothing is the one that hides a permissions
/// problem until the rewrite fails hours later.
fn remove_partial(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[derive(Debug, serde::Serialize)]
struct EmptyParams {}

fn classify(answer: Option<RawResult>, latency: Duration) -> (Outcome, RawResult, Duration) {
    let Some(answer) = answer else {
        return (Outcome::None, truth::null_answer(), latency);
    };
    // Parsed only to decide `resolved` versus `none`; the *stored* value is
    // the bytes the server sent, so replay reads the oracle's answer with the
    // same code the shim reads a live one with.
    let resolved = match serde_json::from_str::<DefinitionResult>(answer.get()) {
        Ok(DefinitionResult::Null) => false,
        Ok(DefinitionResult::One(_)) => true,
        Ok(DefinitionResult::Many(locations)) => !locations.is_empty(),
        Ok(DefinitionResult::Links(links)) => !links.is_empty(),
        Err(error) => {
            tracing::warn!(%error, "a definition answer was not one of the four shapes");
            return (Outcome::Error, truth::null_answer(), latency);
        }
    };
    if resolved {
        (Outcome::Resolved, answer, latency)
    } else {
        (Outcome::None, answer, latency)
    }
}

fn micros(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
}
