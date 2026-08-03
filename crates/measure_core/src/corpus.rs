//! Where the corpus lives (`design/core.md` §7), and the integrity checks that
//! make a run against it mean anything.
//!
//! The layout itself is `data-collection.md` §0's and is not restated here — a
//! directory tree in two documents is a directory tree that will disagree with
//! itself. What this module holds is the consequences: a split is passed by
//! path so that held-out isolation is a filesystem boundary rather than a
//! flag, truth is per server rather than per language, and a checkout is
//! verified rather than trusted.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use shared::{ConfigError, Error, LanguageId};

/// `servers.toml` is at the root of the **code** repository, not in the corpus
/// root: which servers the corpus is collected against is a decision and
/// belongs in the history beside the code that is scored against them, while
/// what the corpus root holds is the several hundred megabytes of installed
/// binaries it points at (`external-dependencies.md` §1,
/// `state/spec-changelog.md` CHANGE-conformance-007).
///
/// Resolved from this crate's own manifest directory rather than from the
/// working directory, because a `measure-<lang>` binary is built from this
/// workspace and a manifest found by searching upward is one that can be
/// shadowed by whatever directory the run happened to start in.
const SERVERS_MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../servers.toml");

/// The workspace lockfile, embedded at build time.
///
/// `core.md` §7 makes the grammar revision part of a truth file's provenance,
/// and the revision is not something the linked grammar can be asked for —
/// `tree_sitter::Language` reports an ABI version, which every grammar built
/// against the same runtime shares. What pins a grammar is the lockfile, so
/// that is what is read. Embedded rather than opened, because the header must
/// name the grammar this binary was *built* with and not whatever the working
/// copy has become since; `rustc` records the include in its dep-info, so
/// re-locking the grammar rebuilds this crate.
const LOCKFILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"));

/// One corpus split, for one language. The two are taken together because
/// every path below is `<split>/<language>/…` and neither half is meaningful
/// alone.
#[derive(Debug)]
pub(crate) struct Corpus {
    split: PathBuf,
    language: LanguageId,
}

#[derive(Clone, Debug)]
pub(crate) struct Repository {
    pub(crate) name: Box<str>,
    pub(crate) path: PathBuf,
}

/// A server as `servers.toml` names it, which is what the provenance header
/// records and what a resumed collection is checked against.
#[derive(Clone, Debug)]
pub(crate) struct ServerEntry {
    pub(crate) name: Box<str>,
    pub(crate) command: Vec<String>,
    pub(crate) version: Box<str>,
}

impl Corpus {
    pub(crate) fn open(split: &Path, language: LanguageId) -> Result<Self, Error> {
        let corpus = Self {
            split: split.to_path_buf(),
            language,
        };
        if !corpus.language_root().is_dir() {
            return Err(ConfigError::CorpusMissing {
                path: corpus.language_root(),
            }
            .into());
        }
        Ok(corpus)
    }

    pub(crate) fn language(&self) -> LanguageId {
        self.language
    }

    fn language_root(&self) -> PathBuf {
        self.split.join(self.language.as_str())
    }

    pub(crate) fn positions(&self, repository: &str) -> PathBuf {
        self.language_root()
            .join("positions")
            .join(format!("{repository}.jsonl"))
    }

    /// Truth is per server, not per language: each server is a different
    /// oracle answering the same questions differently, and refreshing one
    /// must not touch another's.
    pub(crate) fn truth(&self, server: &str, repository: &str) -> PathBuf {
        self.language_root()
            .join("truth")
            .join(server)
            .join(format!("{repository}.jsonl"))
    }

    /// Every repository in the split, or the named subset, in a stable order.
    pub(crate) fn repositories(&self, wanted: &[String]) -> Result<Vec<Repository>, Error> {
        let root = self.language_root().join("repos");
        let mut found = Vec::new();

        if wanted.is_empty() {
            for name in directory_names(&root)? {
                let path = root.join(&*name);
                found.push(Repository { name, path });
            }
        } else {
            for name in wanted {
                let path = root.join(name);
                if !path.is_dir() {
                    return Err(ConfigError::RepositoryMissing { path }.into());
                }
                found.push(Repository {
                    name: name.as_str().into(),
                    path,
                });
            }
        }

        // Sorted rather than in walk order: `replay` is required to be
        // deterministic byte for byte, and directory order is not.
        found.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(found)
    }
}

/// `data-collection.md` §1: `HEAD` matches, and the tree is clean.
///
/// The clean-tree half is the one that actually matters — a modified or extra
/// file changes byte offsets and *does not change `HEAD`* — and untracked
/// files count, because the file list is an `ignore` walk of the filesystem,
/// so an untracked file that is not gitignored is a file the search will find
/// and the truth file has never heard of.
pub(crate) fn verify_checkout(
    repository: &Repository,
    expected: Option<&str>,
) -> Result<Box<str>, Error> {
    let head = git(repository, &["rev-parse", "HEAD"])?;
    let head: Box<str> = head.trim().into();

    if let Some(expected) = expected
        && &*head != expected
    {
        return Err(ConfigError::CommitMismatch {
            repository: repository.path.clone(),
            expected: expected.into(),
            found: head,
        }
        .into());
    }

    if !git(repository, &["status", "--porcelain"])?
        .trim()
        .is_empty()
    {
        return Err(ConfigError::DirtyCheckout {
            repository: repository.path.clone(),
        }
        .into());
    }

    Ok(head)
}

/// `--server <name>` against the code repository's manifest.
pub(crate) fn resolve_server(language: LanguageId, name: &str) -> Result<ServerEntry, Error> {
    let path = PathBuf::from(SERVERS_MANIFEST);
    let text = fs::read_to_string(&path).map_err(|source| ConfigError::ManifestUnreadable {
        path: path.clone(),
        source,
    })?;

    let manifest = manifest::parse(&text, &path)?;
    let servers_root = manifest
        .root
        .get("servers_root")
        .and_then(manifest::Value::as_str)
        .unwrap_or("");

    manifest
        .servers
        .iter()
        .find(|server| {
            server.get("name").and_then(manifest::Value::as_str) == Some(name)
                && server.get("language").and_then(manifest::Value::as_str)
                    == Some(language.as_str())
        })
        .map(|server| ServerEntry {
            name: name.into(),
            command: server
                .get("command")
                .map(|value| value.as_list())
                .unwrap_or_default()
                .iter()
                .map(|word| expand(word, servers_root, &path))
                .collect(),
            version: server
                .get("version")
                .and_then(manifest::Value::as_str)
                .unwrap_or("")
                .into(),
        })
        .ok_or_else(|| {
            ConfigError::UnknownServer {
                manifest: path.clone(),
                name: name.into(),
                language_id: language,
            }
            .into()
        })
}

/// The `grammar` field of the provenance header this build would write.
///
/// Public alongside [`locked_grammar`] because the two claims are different
/// ones: that a pin distinguishes two lockfiles, and that the pin this binary
/// ships names the grammar the workspace declares. The second is only
/// assertable against the embedded lockfile, which is what this reads.
pub fn grammar_pin(language: LanguageId) -> Result<Box<str>, Error> {
    locked_grammar(LOCKFILE, language)
}

/// The locked identity of `language`'s grammar crate, by the crate-name
/// convention every tree-sitter grammar follows: `tree-sitter-<language id>`.
///
/// Public, and taking the lockfile's text rather than reading it, for the same
/// reason `replay_table` is public: the claim is that two different pins
/// produce two different headers, and a function nothing can call with two
/// lockfiles is a claim nothing can assert.
///
/// The revision is the checksum for a registry grammar and the commit for a
/// git one, which are the two shapes a lock entry has. Neither present is an
/// error rather than a shorter pin: a header that names a grammar it cannot
/// identify is the failure this whole field exists to prevent.
pub fn locked_grammar(lockfile: &str, language: LanguageId) -> Result<Box<str>, Error> {
    let package = format!("tree-sitter-{}", language.as_str());
    let Some(locked) = locked_package(lockfile, &package) else {
        return Err(ConfigError::GrammarNotLocked {
            package: package.into(),
        }
        .into());
    };
    let Some(revision) = locked.revision() else {
        return Err(ConfigError::GrammarUnidentified {
            package: package.into(),
        }
        .into());
    };
    Ok(format!("{package} {} ({revision})", locked.version).into_boxed_str())
}

#[derive(Debug)]
struct LockedPackage<'a> {
    version: &'a str,
    checksum: Option<&'a str>,
    source: Option<&'a str>,
}

impl LockedPackage<'_> {
    /// Cargo writes a checksum for a registry package and puts the resolved
    /// commit in the fragment of the source URL for a git one.
    fn revision(&self) -> Option<&str> {
        self.checksum.or_else(|| {
            self.source
                .and_then(|source| source.rsplit_once('#'))
                .map(|(_, revision)| revision)
        })
    }
}

fn locked_package<'a>(lockfile: &'a str, package: &str) -> Option<LockedPackage<'a>> {
    for block in lockfile.split("[[package]]").skip(1) {
        // The entry ends at the next table header of any kind, so a `[metadata]`
        // section after the last package is not read as part of it.
        let block = block.split("\n[").next().unwrap_or(block);
        let (mut name, mut version, mut checksum, mut source) = (None, None, None, None);
        for line in block.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "name" => name = Some(value),
                "version" => version = Some(value),
                "checksum" => checksum = Some(value),
                "source" => source = Some(value),
                _ => {}
            }
        }
        if name == Some(package)
            && let Some(version) = version
        {
            return Some(LockedPackage {
                version,
                checksum,
                source,
            });
        }
    }
    None
}

/// `external-dependencies.md` §1: `servers_root` is relative to the manifest
/// and is expanded into every `${servers}` in the file, so that the same
/// manifest works from any working directory.
fn expand(word: &str, servers_root: &str, manifest: &Path) -> String {
    let root = manifest
        .parent()
        .map(|parent| parent.join(servers_root))
        .unwrap_or_else(|| PathBuf::from(servers_root));
    word.replace("${servers}", &root.to_string_lossy())
}

#[expect(
    clippy::disallowed_methods,
    reason = "`read_dir` is banned because it bypasses gitignore semantics on the *search* path, where a gitignored file is out of scope. This lists corpus checkouts — directories rather than searchable files, one level deep, in a tree the handler never sees — so the rule the ban protects does not reach here, and an `ignore` walk would apply a corpus repository's own .gitignore to the question of whether that repository exists."
)]
fn directory_names(root: &Path) -> Result<Vec<Box<str>>, Error> {
    let entries = fs::read_dir(root).map_err(|source| ConfigError::ManifestUnreadable {
        path: root.to_path_buf(),
        source,
    })?;

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ConfigError::ManifestUnreadable {
            path: root.to_path_buf(),
            source,
        })?;
        if entry.path().is_dir()
            && let Some(name) = entry.file_name().to_str()
        {
            names.push(name.into());
        }
    }
    Ok(names)
}

#[expect(
    clippy::disallowed_methods,
    reason = "`Command::output` is banned because the shim must poll cooperatively against its deadline. `core.md` §7's table gives measure no deadline at all — that is the whole difference between the two programs — so the failure the ban prevents cannot occur here, and `git rev-parse` on a local checkout is bounded by the filesystem rather than by anything that could hang."
)]
fn git(repository: &Repository, arguments: &[&str]) -> Result<String, Error> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(&repository.path)
        .output()
        .map_err(|source| ConfigError::GitUnavailable {
            repository: repository.path.clone(),
            source,
        })?;

    if !output.status.success() {
        return Err(ConfigError::GitUnavailable {
            repository: repository.path.clone(),
            source: std::io::Error::from(std::io::ErrorKind::InvalidData),
        }
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The smallest TOML that reads `servers.toml`, and no more.
///
/// DECISION-conformance-010: provisional. `deps.md` names no TOML parser and
/// the dependency set is a standing Class B trigger, so rather than add one
/// this reads the two shapes the manifest is documented to have — top-level
/// `key = value`, and a `[[server]]` array of tables — and refuses everything
/// else by line number. Confined to this module so that swapping in the `toml`
/// crate is a local change if the escalation is answered that way.
mod manifest {
    use std::path::Path;

    use shared::{ConfigError, Error};

    #[derive(Debug)]
    pub(crate) enum Value {
        Text(String),
        List(Vec<String>),
    }

    impl Value {
        pub(crate) fn as_str(&self) -> Option<&str> {
            match self {
                Value::Text(text) => Some(text),
                Value::List(_) => None,
            }
        }

        pub(crate) fn as_list(&self) -> Vec<String> {
            match self {
                Value::Text(text) => vec![text.clone()],
                Value::List(words) => words.clone(),
            }
        }
    }

    /// A list of pairs rather than a map: a manifest is read once, has a
    /// handful of keys, and a stable order is worth more here than a lookup.
    #[derive(Debug, Default)]
    pub(crate) struct Table(Vec<(String, Value)>);

    impl Table {
        pub(crate) fn get(&self, key: &str) -> Option<&Value> {
            self.0
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
        }

        fn push(&mut self, key: String, value: Value) {
            self.0.push((key, value));
        }
    }

    #[derive(Debug, Default)]
    pub(crate) struct Manifest {
        pub(crate) root: Table,
        pub(crate) servers: Vec<Table>,
    }

    pub(crate) fn parse(text: &str, path: &Path) -> Result<Manifest, Error> {
        let mut manifest = Manifest::default();
        let mut in_server = false;

        for (index, line) in text.lines().enumerate() {
            let line = strip_comment(line).trim();
            if line.is_empty() {
                continue;
            }
            if line == "[[server]]" {
                manifest.servers.push(Table::default());
                in_server = true;
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ConfigError::ManifestMalformed {
                    path: path.to_path_buf(),
                    line: index + 1,
                }
                .into());
            };
            let (key, value) = (key.trim().to_owned(), literal(value.trim()));
            match manifest.servers.last_mut() {
                Some(server) if in_server => server.push(key, value),
                Some(_) | None => manifest.root.push(key, value),
            }
        }

        Ok(manifest)
    }

    /// A `#` inside a quoted string is not a comment. Tracking that is the
    /// difference between this and a `split_once('#')`, and paths under
    /// `${servers}` are exactly where a `#` would show up.
    fn strip_comment(line: &str) -> &str {
        let mut quoted = false;
        for (index, byte) in line.bytes().enumerate() {
            match byte {
                b'"' => quoted = !quoted,
                b'#' if !quoted => return line.split_at(index).0,
                _ => {}
            }
        }
        line
    }

    fn literal(text: &str) -> Value {
        if let Some(inner) = text
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            return Value::List(
                inner
                    .split(',')
                    .map(str::trim)
                    .filter(|word| !word.is_empty())
                    .map(unquote)
                    .collect(),
            );
        }
        Value::Text(unquote(text))
    }

    fn unquote(text: &str) -> String {
        text.trim()
            .trim_start_matches('"')
            .trim_end_matches('"')
            .to_owned()
    }
}
