# A heuristic go-to-definition LSP shim

Ever stuck waiting for the LSP, wanting to do a "go-to-definition"?
This is a tool to provides imprecise results when the proper LSP isn't
ready.

It can simply be run like this:

```sh
$ heuristic-jump rust-analyzer
```

Each language implements its own resolution logic, but shares a lot of
code for similar patterns.

## Implementation

This will store the current state of the project files, as much as
needed by the LSP protocol.

For recently queried and/or recently edited files, it will have an
in-memory tree-sitter parse (that is incrementally updated)

When a go-to-definition request is received, run the proper LSP. If a
query is done again on the same spot and it still hasn't responded,
give the heuristic one. Also complete the proper LSP request with the
heuristic one.

If the proper LSP returns after a heuristic result is used, but its
result is different, notify the user.

Steps:

* Collect the id and namespace under the cursor, and information about
  the usage context (Class vs function vs type vs type variable, etc)?

* Walk up doing proper local binding resolution.

* Info from the current file's imports is used - if it's explicitly
  imported, then the corresponding file may be able to be found
  directly.

    - If found, a technique like
    https://github.com/jacktasia/dumb-jump is used to find candidate
    definitions there.

    - A tree-sitter parse of the found candidates is done, and it
      checks if the candidate parses (not in a block comment,
      etc). For languages that have Some.Object.Nested, it's analyzed
      whether it would be been possible for this to be the referenced
      thing.

    - If not found, fall back to whole project search.

* If it may be part of wildcard imports, then dumb-jump style search
  is used on all of the wildcard-imported modules.

* Otherwise, just search the whole project.

## Future questions

1. When it's whole project search, how to choose the better module when
it's heuristic ? Maybe something like repomap's pagerank?

## Development plan

The plan is to have ~10 opensource repos per language, and
incrementally collect authoritative go-to-definition information from
the LSP. There will be a complete scan of all identifiers, and it will
write all of it to a file.

Claude code sessions will then be used on each language to improve its
match rate vs the proper LSP behavior.

## Prior version

I have an old version of this in ../heuristic_jump_old, and may want to
use the text similarity stuff, maybe other things.  However, it was
based on the idea of being integrated directly into Zed, where
language configs are more traditionally based on treesitter queries.
I no longer want to stick with that limitation.  Instead, each
language will have its own analysis implementation in rust.

Generally this should reference the zed code (../zed) and how it does
LSP. It's conceivable this could be integrated back into Zed or
possibly as an extension if Zed's extension API is greatly expanded.
