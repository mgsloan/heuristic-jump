# External dependencies

Phase 1c of [`phases.md`](phases.md): every language server the corpus is
collected against, how it was installed, and what it needs at run time. This
is the human half. The machine-readable half is `servers.toml` at the root of
this repository, which is what `measure collect` resolves `--server <name>`
against; the split exists because `collect` must record what it actually ran
and a prose document cannot be resolved by a program
([`data-collection.md` §0](data-collection.md)).

Also pinned here: the tools that produce the numbers but appear in no
`Cargo.toml` — the Claude Code CLI most of all, for the reason
[`loops.md` §17](loops.md) gives.

## 1. Where it all lives, and why not on the system

It is split across the repository and the corpus root, along the line of what
is reviewable:

```
heuristic_jump/
  servers.toml                    the matrix, machine-readable
  harness/verify-servers          the acceptance check
  harness/server-fixtures/<lang>/ the probes it runs

../heuristic-jump-corpus/servers/
  rust/rust-analyzer              standalone binary
  go/bin/gopls                    go install --> GOBIN
  c/clangd_22.1.6/bin/clangd      unpacked release
  python/pylsp-venv/              uv venv
  node/<server>/node_modules/     one container per server
```

Which servers the corpus is collected against is a decision, and it belongs
in the history beside the code that is scored against it — as does the check
that says the decision was carried out. 472 MB of platform-specific binaries
is not a decision, and it lives beside the corpus it serves.

The manifest resolves the two halves with a `servers_root` key, relative to
itself, expanded into every `${servers}` in the file. `HJ_SERVERS` overrides
it. Nothing is on `$PATH`.

**The verifier and the fixtures are under `harness/`, which every loop is
denied.** `servers.toml` is not in any loop's write list either, though that
is an allowlist rather than the deny list's constant. Both facts are the same
argument: this manifest names the oracle a language loop is measured against,
and a loop that could edit it could choose its own examiner.

**Nothing is installed system-wide, and no `sudo` was used.** The reason is
not tidiness. `data-collection.md` §4 makes the server version part of every
truth file's provenance header and has `collect` refuse to resume a file
whose server has moved underneath it. A version that an unrelated
`apt upgrade` can change is not a pin, and the drift check would spend its
life reporting drift nobody chose. Every server here is therefore installed
at an exact version into a directory this project owns, and upgrading one is
a deliberate act.

**The install tree is platform-specific.** `clangd`, `rust-analyzer` and
`gopls` are x86_64 Linux binaries sitting inside what is otherwise portable
data. Moving the corpus to another machine means re-running section 3 and
re-running `harness/verify-servers`; it does not mean re-collecting anything.

## 2. What the host must already provide

| Tool | Version here | Needed by |
|---|---|---|
| `node` | v24.18.1 | pyright, basedpyright, typescript-language-server, vtsls |
| `npm` | 11.16.0 | installing the above |
| `go` | go1.26.5 | building gopls, and gopls at run time |
| `python3` | 3.14.6 | `harness/verify-servers` (needs `tomllib`, so ≥3.11) |
| `uv` | 0.11.8 | the pylsp venv |
| `cargo` | 1.95.0 | rust-analyzer shells out to it for `cargo metadata` |

These are the machine's, not the project's, and they are recorded because
they are inputs to the oracle even though nothing pins them. `gopls` is the
sharpest case: it is a Go program that also *invokes* the Go toolchain, so
the host `go` version participates in its answers.

C and C++ need a system compiler for the `compile_commands.json` step
([§6](#6-what-each-server-needs-from-a-repository)), not for clangd itself —
the clangd release bundles its own libclang. `gcc` 15.3.0 and `clang` 21.1.8
are what happen to be here.

## 3. The servers

Eight servers over seven languages. The set is "every trustworthy server Zed
supports for the language", which is `data-collection.md` §4's rule; §4 of
this document says what that rule excluded and why.

Each pin below is the identity the install was requested by. What the
installed server *reports* is a separate string, recorded in `servers.toml`
as `version` because that is what the drift check compares — see
[§5](#5-verification).

### rust-analyzer — Rust

```
curl -sSL -o rust-analyzer.gz \
  https://github.com/rust-lang/rust-analyzer/releases/download/2026-08-03/rust-analyzer-x86_64-unknown-linux-gnu.gz
gunzip rust-analyzer.gz && chmod +x rust-analyzer
```

Pin `2026-08-03`, reporting `rust-analyzer 0.3.2997-standalone`.
`sha256(rust-analyzer.gz) = 769670319df8571dac91b6eab6d3a65b18b69488a6900959f2fb6157181ace9d`.

**The standalone release rather than the rustup component**, which is the one
place this deliberately diverges from Zed — Zed's `rust.rs` tries
`rustup which rust-analyzer` first and only falls back to the GitHub release.
Two reasons, and the second is the load-bearing one:

* The rustup component tracks the compiler. `rust-toolchain.toml` pins 1.95.0,
  whose rust-analyzer is from 2026-04-14; the standalone release is current.
* `~/.cargo/bin/rust-analyzer` is a *proxy*, and it resolves the toolchain
  from the directory it is invoked in. Run it inside a corpus repository that
  carries its own `rust-toolchain.toml` and it silently becomes a different
  rust-analyzer — a per-repository oracle version, which is precisely what
  the provenance header exists to make impossible. A concrete binary cannot
  do that.

The second reason also means a toolchain bump — already an intervention,
per `rust-toolchain.toml` — no longer moves the oracle as a side effect.
`cargo` is still invoked by rust-analyzer for `cargo metadata`, and there the
repository's own toolchain pin applies, which is what we want.

### gopls — Go

```
GOBIN=$PWD/bin go install golang.org/x/tools/gopls@v0.23.0
```

Pin `v0.23.0`, reporting `golang.org/x/tools/gopls v0.23.0`.

Built from source by the host toolchain, so this binary is a function of both
the module version and `go1.26.5`. Its `serverInfo.version` is its entire
build manifest — a few kilobytes of JSON listing every dependency — which is
excellent provenance and unreadable in a table, so the drift check reads
`gopls version` instead.

### clangd — C, C++

```
curl -sSL -o clangd.zip \
  https://github.com/clangd/clangd/releases/download/22.1.6/clangd-linux-22.1.6.zip
unzip clangd.zip
```

Pin `22.1.6`, reporting
`clangd version 22.1.6 (https://github.com/llvm/llvm-project fc4aad7b5db3fff421df9a9637605b9ca5667881)`.
`sha256(clangd.zip) = a9c77443af2e447ed467e84771848d3a6ac1c56f84bcfcde717e66318de77cfa`.

Same source Zed's `c.rs` uses. The release is self-contained: it does not use
the system LLVM, and the version string carries the upstream commit, so the
provenance header identifies the exact llvm-project tree.

### pyright, basedpyright — Python

```
npm install pyright@1.1.411          # in node/pyright/
npm install basedpyright@1.39.9      # in node/basedpyright/
```

Invoked as `node <container>/node_modules/<pkg>/langserver.index.js --stdio`,
which is Zed's `SERVER_PATH` for both.

**They are in separate container directories, and that is not cosmetic.**
`basedpyright` is a pyright fork and it declares npm bin names `pyright` and
`pyright-langserver` as well as its own. Install both into one `node_modules`
and `.bin/pyright --version` answers `basedpyright 1.39.9` — npm resolves the
collision by last writer, silently. The servers themselves were unaffected,
since they are invoked by explicit path, but the *drift check* reads
`version_command`, so a shared tree would have had one of the two servers
permanently reporting the other's version into a provenance header. One
container per server is also what Zed does, for its own reasons; here it is
load-bearing.

Neither exposes `--version` on the langserver entrypoint — it fails with
"Connection input stream is not set" — so `version_command` uses the CLI bin
inside the same container.

### pylsp — Python

```
uv venv --python 3.13 pylsp-venv
uv pip install python-lsp-server==1.15.0
```

Pin `1.15.0`, reporting `pylsp v1.15.0`.

A dedicated venv rather than `uv tool install`, so the interpreter and every
transitive dependency are pinned in one place this project owns. pylsp
resolves through jedi rather than a type checker, which is why it earns a
place next to two pyrights: it is a genuinely different resolution strategy,
and a heuristic that agrees with both is more interesting than one that
agrees with either.

### typescript-language-server, vtsls — JavaScript, TypeScript/TSX

```
npm install typescript@6.0.3 typescript-language-server@5.3.0   # one container
npm install @vtsls/language-server@0.3.0                        # another
```

Both are invoked as `node <entrypoint> --stdio`.

**TypeScript is pinned to 6.0.3, not the `latest` tag, which is 7.0.2.**
TypeScript 7 is the Go rewrite; Zed constrains the package to `^6` with the
comment "Pin rather than Latest so an unusable TypeScript 7.x install gets
downgraded", and 6.0.3 is the newest stable release satisfying that. This is
a pin inside a pin: `typescript-language-server` drives `tsserver` out of the
`typescript` package, so the TypeScript version is as much a part of the
oracle as the server version, and it does not appear in the server's own
version string. vtsls bundles its own TypeScript and needs no companion pin.

## 4. What is deliberately not installed

**ccls** — listed in `data-collection.md` §4's original matrix and removed
from it by this phase. Zed has no ccls adapter: not built in, and no
extension in the registry. §4 defines the set as "every trustworthy server
Zed supports", so ccls fails the rule the table was meant to express; it also
needs a from-source build against LLVM, and it is barely maintained upstream.
C and C++ are collected against clangd alone. Logged as
`spec-edited-by-hand` in `state/interventions.jsonl`.

**ty** — Zed built-in, Astral's checker, and a real candidate later. Excluded
now at 0.0.65: pre-alpha versioning makes the pin a moving target, and a
server whose definition behaviour changes between patch releases is a poor
oracle for a corpus that is supposed to be frozen.

**ruff, eslint** — Zed built-in and both irrelevant here. Neither answers
`textDocument/definition`, which is the only request the corpus consists of.

**basedpyright** was added, not removed: it is Zed built-in and postdates
`data-collection.md`'s table. It is a pyright fork, so expect its answers to
correlate strongly with pyright's; the cost is one collection pass and the
information is whether the fork has diverged where it matters.

## 5. Verification

`harness/verify-servers` is the phase's acceptance check. Per server, per
language it claims: read `version_command` and compare against the manifest,
then start the server, `initialize`, `didOpen` a fixture, and issue
`textDocument/definition` at a call site whose definition is three lines up.
It passes only if the server both advertises `definitionProvider` and returns
the right line.

```
$ harness/verify-servers
PASS  rust-analyzer/rust                    ...  definition -> line 0, expected 0
PASS  gopls/go                              ...  definition -> line 4, expected 4
PASS  clangd/c                              ...  definition -> line 0, expected 0
PASS  clangd/cpp                            ...  definition -> line 0, expected 0
PASS  pyright/python                        ...  definition -> line 0, expected 0
PASS  basedpyright/python                   ...  definition -> line 0, expected 0
PASS  pylsp/python                          ...  definition -> line 0, expected 0
PASS  typescript-language-server/javascript ...  definition -> line 0, expected 0
PASS  typescript-language-server/typescript ...  definition -> line 0, expected 0
PASS  vtsls/javascript                      ...  definition -> line 0, expected 0
PASS  vtsls/typescript                      ...  definition -> line 0, expected 0
```

11 probes, 8 version checks, all green on 2026-08-02.

It is deliberately weaker than `measure collect` will be — no readiness
protocol, single-file fixtures, no dependency resolution — because its
question is only "is this install usable at all", asked before a hundred
machine-hours find out that it is not. Two things it caught are worth
recording, because both would have been near-invisible later:

* **The npm bin collision** in [§3](#pyright-basedpyright--python).
* **rust-analyzer answers `-32801 ContentModified` for the whole of its first
  workspace load**, and only then starts answering definitions. It is the
  readiness crux from `data-collection.md` §4 arriving on the smallest
  possible input — a two-file cargo project — and a client that treated the
  error as an answer would record a `none` for a position with a perfectly
  good definition. `verify-servers` retries on `-32801` and `-32800`; whatever
  `measure collect` does about readiness, treating these two codes as "ask
  again" rather than as an outcome is not optional.

**The Rust fixture declares an empty `[workspace]`**, which is not decoration.
It lives under `harness/` and would otherwise be a package inside this
repository's workspace directory without being a member, which `cargo
metadata` refuses outright — so rust-analyzer loads nothing and answers
nothing, and the failure looks like a broken server rather than a broken
fixture. Making it its own root also keeps it out of the gate's build, lint
and format sweep, which is where it belongs: it is a thing for a server to
load, not a crate this project ships.

## 6. What each server needs from a repository

`data-collection.md` §1 says dependency resolution is part of repository
*selection* and is the phase's largest practical risk. This is the same table
from the server's side — the knob each one needs pointed at the right place
once phase 1b has produced a resolvable checkout. None of it is configured
yet; there are no repositories.

| Server | Needs |
|---|---|
| rust-analyzer | `cargo metadata` succeeding, and a populated registry. Loads the workspace itself |
| gopls | modules downloaded; `GOFLAGS=-mod=mod`, `GOPATH`/`GOMODCACHE` inherited from the host |
| clangd | `compile_commands.json`, via `--compile-commands-dir` if it is not at the root. The hardest of the set |
| pyright, basedpyright | the interpreter, via `python.pythonPath` / `venvPath` in the initialization options |
| pylsp | the interpreter, via `pylsp.plugins.jedi.environment` — jedi resolves imports against *its own* `sys.path` unless told otherwise, so an unset environment resolves against the pylsp venv and finds nothing the repository declares |
| typescript-language-server | `node_modules` installed and a `tsconfig.json`. `--tsserver-path` is unnecessary here because `typescript` sits beside it in the container |
| vtsls | the same, minus the TypeScript question |

The pylsp row is the trap: it fails *quietly*, resolving to a venv that
contains the language server and nothing else, which produces exactly the
systematically-biased truth file §1 describes.

## 7. The Claude Code CLI

**2.1.220**, which is what every campaign to date has run under.

It is pinned here because it belongs to the same category as the servers and
the toolchain: an input that changes the numbers with no diff in this
repository explaining it. `loops.md` §17 makes the argument — a CLI upgrade
changes the *generator* of campaigns exactly as a prompt revision does,
arrives without anyone deciding anything, and is otherwise completely
invisible. Metrics either side of an upgrade are not strictly comparable.

Each campaign already records the version it ran under, so the join exists.
What this line adds is the decision to treat a change as an intervention:
log it with `harness/hj intervene --kind cli-upgraded`.
