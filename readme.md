# A heuristic go-to-definition LSP shim

Ever stuck waiting for the LSP, wanting to do a "go-to-definition"?
This is a tool that provides imprecise results when the proper LSP
isn't ready.

It can simply be run like this:

```sh
$ heuristic-jump -- rust-analyzer
```

It also runs with no language server behind it at all - just
leave off the `-- rust-analyzer` part:

```sh
$ heuristic-jump
```
