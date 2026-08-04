//! `measure enumerate`: positions are enumerated **once per repository**, not
//! once per server (`data-collection.md` §2).
//!
//! That is what makes cross-server comparison possible at all. If each server
//! run enumerated its own positions, two servers' answers could not be
//! aligned, and the agreement/divergence split `core.md` §7 builds the whole
//! per-server design on would have nothing to join on.
//!
//! Which positions count is [`shared::identifiers`] and not a rule of this
//! module's own — the same function a handler answers `NotAnIdentifier` with.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::{Deserialize, Serialize};
use shared::{
    CodecError, ConfigError, Deadline, DocumentUri, DocumentVersion, Error, LanguageHandler,
    LanguageId, Offset, ProjectPath, Rope, SnapshotSeed,
};

/// `(file, byte offset, text, node kind, class)`.
///
/// `class` is **not** a stratum: strata are defined by the resolution logic
/// and do not exist at collection time, which is the circularity
/// `data-collection.md` §3 refuses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Position {
    pub(crate) file: Box<str>,
    pub(crate) offset: usize,
    pub(crate) text: Box<str>,
    pub(crate) kind: Box<str>,
    pub(crate) class: Class,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Class {
    Identifier,
    /// Keywords, punctuation, string interiors, comments: places a user can
    /// press go-to-definition where the honest answer is nothing. They carry
    /// their own denominator and never enter the main one — on these positions
    /// answering nothing is correct, so folding them into coverage would mix
    /// two different questions.
    Other,
}

/// `data-collection.md` §2: about 100 non-identifier positions per language in
/// total, not per repository. They exist to prove the `NotAnIdentifier` path
/// fires on real input, not to measure anything, so a hundred is plenty and
/// the sample does not need to be representative.
const NON_IDENTIFIER_TOTAL: usize = 100;

pub(crate) fn read(path: &Path) -> Result<Vec<Position>, Error> {
    let file = File::open(path).map_err(|_| ConfigError::ArtifactMissing {
        path: path.to_path_buf(),
    })?;

    let mut positions = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|source| ConfigError::ManifestUnreadable {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        positions.push(serde_json::from_str::<Position>(&line).map_err(|source| {
            ConfigError::ArtifactMalformed {
                path: path.to_path_buf(),
                line: index + 1,
                source,
            }
        })?);
    }
    Ok(positions)
}

pub(crate) fn write(path: &Path, positions: &[Position]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::ManifestUnreadable {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut text = String::new();
    for position in positions {
        text.push_str(&serde_json::to_string(position).map_err(|source| {
            CodecError::NotSerializable {
                what: "a corpus position",
                source,
            }
        })?);
        text.push('\n');
    }
    fs::write(path, text).map_err(|source| ConfigError::ManifestUnreadable {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Every candidate position in one repository, before sampling.
///
/// The offset is the identifier's *start*. A cursor can sit anywhere inside a
/// token and the handler must behave identically wherever it sits, but that
/// invariance is better asserted by a property test than paid for with corpus
/// positions.
/// `language` is passed rather than looked up: `measure_core::run` has already
/// resolved which language this binary is about, and a second lookup here was
/// a second answer to that question — one that fell back to
/// `LanguageId::new("unknown")` where the caller refuses.
pub(crate) fn enumerate(
    handler: &dyn LanguageHandler,
    language: LanguageId,
    files: &[ProjectPath],
    non_identifiers_wanted: usize,
) -> Result<Vec<Position>, Error> {
    // Enumeration is offline corpus work with no user waiting on it, and a
    // wall-clock deadline here would make the *position set* depend on machine
    // load — which is the one thing `data-collection.md` §2 needs stable, since
    // every server run afterwards is joined on it.
    let deadline = Deadline::none();
    let mut found = Vec::new();
    let mut non_identifiers = 0;

    for path in files {
        let absolute = path.to_absolute();
        let Ok(text) = fs::read_to_string(&absolute) else {
            // A file that cannot be read as UTF-8 is not one the handler could
            // have searched either. Skipping it costs positions, which is a
            // smaller error than a corpus that cannot be regenerated.
            tracing::debug!(path = %absolute.display(), "skipping an unreadable file");
            continue;
        };
        let Some(uri) = DocumentUri::from_file_path(&absolute) else {
            continue;
        };
        let relative: Box<str> = path.rel().as_path().to_string_lossy().into();

        let document = SnapshotSeed::fresh(
            uri,
            Rope::from(text.as_str()),
            DocumentVersion(0),
            language,
            handler.grammar(),
        )
        .realise(&deadline)?;

        for node in shared::identifiers(&document) {
            let Some(slice) = text.get(node.start_byte()..node.end_byte()) else {
                continue;
            };
            found.push(Position {
                file: relative.clone(),
                offset: node.start_byte(),
                text: slice.into(),
                kind: node.kind().into(),
                class: Class::Identifier,
            });
        }

        if non_identifiers < non_identifiers_wanted {
            let taken = take_non_identifiers(
                &document,
                &text,
                &relative,
                non_identifiers_wanted - non_identifiers,
            );
            non_identifiers += taken.len();
            found.extend(taken);
        }
    }

    Ok(found)
}

/// The other denominator: offsets [`shared::identifier_at`] declines.
///
/// Derived from the same function the handler answers with, so "a position the
/// corpus considers a query" and "a position the tool considers a query" are
/// the same set by construction rather than by two lists agreeing.
fn take_non_identifiers(
    document: &shared::DocumentSnapshot,
    text: &str,
    relative: &str,
    wanted: usize,
) -> Vec<Position> {
    let mut found = Vec::new();
    // A stride proportional to the file rather than a fixed one, so a small
    // file contributes probes at all: these are ~100 per *language* in total
    // and their only job is to prove the `NotAnIdentifier` path fires on real
    // input, so spreading them thinly over whatever files exist is exactly
    // right and a fixed stride silently yields nothing on a short file.
    let stride = text.len().div_ceil(16).max(1);
    for offset in (0..text.len()).step_by(stride) {
        if found.len() >= wanted {
            break;
        }
        if !text.is_char_boundary(offset)
            || shared::identifier_at(document, Offset(offset)).is_some()
        {
            continue;
        }
        let end = text[offset..]
            .char_indices()
            .nth(1)
            .map_or(text.len(), |(width, _)| offset + width);
        let Some(slice) = text.get(offset..end) else {
            continue;
        };
        found.push(Position {
            file: relative.into(),
            offset,
            text: slice.into(),
            kind: kind_at(document, Offset(offset)),
            class: Class::Other,
        });
    }
    found
}

fn kind_at(document: &shared::DocumentSnapshot, offset: Offset) -> Box<str> {
    document
        .tree()
        .root_node()
        .descendant_for_byte_range(offset.0, offset.0)
        .map_or_else(|| "".into(), |node| node.kind().into())
}

/// Uniform random, capped. Not stratified: strata are defined by *where the
/// definition turned out to be*, which is exactly what is not known before the
/// LSP answers, and pre-classifying with our own logic would make the corpus's
/// labels come from the code under measurement.
pub(crate) fn sample(mut positions: Vec<Position>, limit: usize, seed: u64) -> Vec<Position> {
    if positions.len() > limit {
        let mut random = SplitMix64::new(seed);
        // Partial Fisher-Yates: the first `limit` slots become a uniform
        // sample, and nothing beyond them is touched.
        for index in 0..limit {
            let span = u64::try_from(positions.len() - index).unwrap_or(u64::MAX);
            let pick = index + usize::try_from(random.next() % span.max(1)).unwrap_or(0);
            positions.swap(index, pick);
        }
        positions.truncate(limit);
    }
    // Sorted after sampling, not before: the sample is what the seed decides,
    // and the order it is written in is what makes two runs byte-identical.
    positions.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.offset.cmp(&right.offset))
    });
    positions
}

/// The ~100 per language, taken from the same files by the same seed.
///
/// A non-identifier position is any byte offset that [`shared::identifier_at`]
/// declines — which is the point: the corpus's definition of "not a query" and
/// the handler's are the same function, so a disagreement is not
/// representable.
pub(crate) fn non_identifier_quota(repositories: usize) -> usize {
    if repositories == 0 {
        return 0;
    }
    NON_IDENTIFIER_TOTAL.div_ceil(repositories)
}

/// SplitMix64, written out rather than depended on: the sample has to be
/// reproducible from a seed, and that is the whole requirement. `rand` is in
/// the workspace for `vendor/rope`'s upstream tests and is not on `deps.md`'s
/// list for anything of ours.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}
