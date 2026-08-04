#!/usr/bin/env python3
"""Normalise a raw editor capture into `core.md` §8.5's golden corpus.

Framed LSP goes in, one JSON object per line comes out — the same shape
`crates/shared/tests/golden-traffic.jsonl` already holds, so the differential
test needs no change to pick these up.

Two things are deliberate. **Payloads are kept verbatim**, because the corpus
exists to catch a `#[serde(untagged)]` variant ordered wrongly and a
normalised payload is one that has already been through somebody's idea of the
shape. And **duplicates are dropped by structure rather than by bytes**: a
minute of typing produces hundreds of `didChange` notifications that differ
only in offsets and text, and the corpus wants one of each *shape*.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def frames(text: str):
    """Split `Content-Length`-framed LSP out of a raw stream."""
    offset = 0
    while True:
        header_end = text.find("\r\n\r\n", offset)
        if header_end < 0:
            return
        header = text[offset:header_end]
        length = None
        for line in header.splitlines():
            if line.lower().startswith("content-length:"):
                try:
                    length = int(line.split(":", 1)[1])
                except ValueError:
                    return
        if length is None:
            return
        body = text[header_end + 4 : header_end + 4 + length]
        offset = header_end + 4 + length
        try:
            yield json.loads(body)
        except json.JSONDecodeError:
            continue


def shape(value, depth: int = 0):
    """The structure, with scalars replaced by their type.

    Two `didChange` notifications differ in every offset and are the same
    message as far as a deserialiser is concerned. What distinguishes an
    untagged variant is which keys are present, so that is what is kept.
    """
    if depth > 12:
        return "..."
    if isinstance(value, dict):
        return {key: shape(value[key], depth + 1) for key in sorted(value)}
    if isinstance(value, list):
        return [shape(value[0], depth + 1)] if value else []
    return type(value).__name__


def main() -> int:
    raw_dir, corpus_path = Path(sys.argv[1]), Path(sys.argv[2])
    existing = []
    if corpus_path.exists():
        existing = [
            json.loads(line) for line in corpus_path.read_text().splitlines() if line.strip()
        ]
    seen = {json.dumps(shape(row.get("message", row)), sort_keys=True) for row in existing}

    added, scanned = [], 0
    for path in sorted(raw_dir.glob("*.jsonl")):
        direction = "client" if path.name.startswith("client-") else "server"
        for message in frames(path.read_text(errors="replace")):
            scanned += 1
            key = json.dumps(shape(message), sort_keys=True)
            if key in seen:
                continue
            seen.add(key)
            added.append({"from": direction, "captured": "editor", "message": message})

    if not added:
        print(f"capture: {scanned} frame(s) scanned, none of a shape the corpus lacked")
        return 0

    with corpus_path.open("a") as out:
        for row in added:
            out.write(json.dumps(row, sort_keys=True) + "\n")

    kinds: dict[str, int] = {}
    for row in added:
        name = row["message"].get("method") or ("response" if "result" in row["message"] else "?")
        kinds[name] = kinds.get(name, 0) + 1
    print(f"capture: {scanned} frame(s) scanned, {len(added)} new shape(s) added")
    for name, count in sorted(kinds.items(), key=lambda pair: -pair[1]):
        print(f"  {count:3}  {name}")
    print(f"\nNow re-audit: the client half of core.md §8.5 is no longer missing.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
