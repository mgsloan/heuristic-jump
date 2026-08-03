#!/usr/bin/env python3
"""Vendor the LSP specification, resolved and trimmed.

The published page is Jekyll: the 3.17 `specification.md` is a shell of ninety-
odd includes, so fetching it gets 42KB of directives rather than the protocol.
This resolves them, then writes a second copy with everything the shim does not
touch removed — the specification's own text with sections cut, never a
paraphrase, because a summary of a protocol is a second source of truth and the
point of vendoring is to have one.

    harness/reference-lsp [--version 3.17]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
UPSTREAM = "https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages"

# What the shim proxies through, plus the one family it answers itself.
KEEP: list[tuple[str, list[str]]] = [
    ("The wire", ["baseProtocol", "basicJsonStructures"]),
    ("How messages relate", ["capabilities", "messageOrdering"]),
    ("Lifecycle", ["lifeCycleMessages"]),
    ("Document synchronisation", ["textDocument_synchronization"]),
    (
        "The definition family",
        [
            "textDocument_definition",
            "textDocument_declaration",
            "textDocument_typeDefinition",
            "textDocument_implementation",
        ],
    ),
]

HEADING = re.compile(r'^(#{2,6})\s+<a href="#([^"]+)"[^>]*>(.*?)</a>', re.M)
INCLUDE = re.compile(r"\{%\s*(include_relative|include)\s+([^%]+?)\s*%\}")


def fetch(path: str) -> str | None:
    result = subprocess.run(
        ["curl", "-sS", "--fail", "--max-time", "60", f"{UPSTREAM}/{path}"],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.stdout if result.returncode == 0 else None


def resolve(text: str, spec_root: str, seen: set[str], missing: list[str], depth: int = 0) -> str:
    """`include_relative` is rooted at the version directory, `include` at
    `_includes/`. Both appear, and getting them the same way silently drops
    half the document."""
    if depth > 8:
        return text

    def substitute(match: re.Match[str]) -> str:
        kind, relative = match.group(1), match.group(2).strip()
        path = f"{spec_root}/{relative}" if kind == "include_relative" else f"_includes/{relative}"
        if path in seen:
            return ""
        seen.add(path)
        body = fetch(path)
        if body is None:
            missing.append(path)
            return f"\n<!-- unresolved include: {relative} -->\n"
        return "\n" + resolve(body, spec_root, seen, missing, depth + 1) + "\n"

    return INCLUDE.sub(substitute, text)


def cut(full: str, anchor: str) -> str | None:
    """One section, heading through the next heading of the same or higher
    level — so taking a section takes everything beneath it."""
    marks = [(m.start(), len(m.group(1)), m.group(2)) for m in HEADING.finditer(full)]
    for index, (start, level, name) in enumerate(marks):
        if name != anchor:
            continue
        for later_start, later_level, _ in marks[index + 1 :]:
            if later_level <= level:
                return full[start:later_start].rstrip()
        return full[start:].rstrip()
    return None


PREAMBLE = """# LSP {version}, trimmed to what the shim touches

Not a summary and not a paraphrase — every section below is the specification's
own text, unedited, with the rest removed. `specification.md` beside this file
is the whole thing and is the authority; this exists so that reading the
protocol costs a few thousand tokens rather than four hundred thousand.

**What is here, and why.** The shim is a proxy: it forwards every message it
does not answer, so it needs the base protocol exactly — framing, request and
response shapes, error codes, cancellation, ordering — and it needs document
synchronisation exactly, because it holds authoritative text for every open
document. It needs the definition family in full because that is the one thing
it answers itself.

**What is not here.** Every other language feature: completion, hover, code
actions, semantic tokens, inlay hints, formatting, rename, references, call and
type hierarchy, diagnostics, folding, symbols, notebooks, and the workspace and
window features. The shim forwards all of them byte-identically without
inspecting them (`shim.md` §3 argues how little inspection the forwarding path
needs), so their payload shapes are not its business. If a task ever needs one,
it is in `specification.md`.

**Generated** by `harness/reference-lsp`. Do not edit: regenerate.
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", default="3.17")
    args = parser.parse_args()

    spec_root = f"_specifications/lsp/{args.version}"
    out = REPO / "reference" / f"lsp-{args.version}"
    out.mkdir(parents=True, exist_ok=True)

    shell = fetch(f"{spec_root}/specification.md")
    if shell is None:
        print(f"reference-lsp: cannot fetch {spec_root}/specification.md", file=sys.stderr)
        return 1

    seen: set[str] = set()
    missing: list[str] = []
    full = resolve(shell, spec_root, seen, missing)
    full = re.sub(r"\{%[^%]*%\}", "", full)  # Jekyll leftovers carry no content
    (out / "specification.md").write_text(full)
    print(f"reference-lsp: {len(seen)} includes resolved, {len(missing)} missing, {len(full):,} bytes")
    for path in missing:
        print(f"  missing: {path}", file=sys.stderr)

    meta = fetch(f"{spec_root}/metaModel/metaModel.json")
    if meta is not None:
        (out / "metaModel.json").write_text(meta)
        model = json.loads(meta)
        print(
            f"reference-lsp: metaModel {len(model.get('requests', []))} requests, "
            f"{len(model.get('notifications', []))} notifications, "
            f"{len(model.get('structures', []))} structures"
        )

    pieces = [PREAMBLE.format(version=args.version)]
    absent: list[str] = []
    for group, anchors in KEEP:
        pieces.append(f"\n\n<!-- ==================== {group} ==================== -->\n")
        for anchor in anchors:
            body = cut(full, anchor)
            if body is None:
                absent.append(anchor)
                continue
            pieces.append(body + "\n")
    trimmed = "\n".join(pieces)
    (out / "shim-relevant.md").write_text(trimmed)
    print(
        f"reference-lsp: trimmed to {len(trimmed):,} bytes "
        f"({len(trimmed) / len(full) * 100:.0f}% of the whole)"
    )
    if absent:
        print(f"reference-lsp: anchors not found: {', '.join(absent)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
