**The protocol is vendored — read it rather than recalling it.**
   `reference/lsp-3.17/shim-relevant.md` is LSP 3.17 with everything the shim
   does not touch removed: the base protocol, lifecycle, document
   synchronisation and the definition family, in the specification's own words.
   `reference/lsp-3.17/specification.md` beside it is the whole thing, and
   `metaModel.json` is every request and structure machine-readable.

   Use them whenever a claim turns on what the protocol actually says — a
   field's optionality, an error code, what a server may send unsolicited.
   Recalling the protocol from memory is how a `#[serde(untagged)]` variant
   ends up in the wrong order, which `core.md` §8.5 spends a section on. You
   may not edit anything under `reference/`; it is generated.
