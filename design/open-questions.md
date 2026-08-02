1. When it's whole project search, how to choose the better module when
   it's heuristic? Maybe something like repomap's pagerank?

2. Does the editor actually send a second go-to-definition request while the
   first is still pending? It might instead cancel the first and send a new one,
   or dedupe and send nothing at all. The entire retry-triggered design assumes
   the first of these. This needs a trace from Zed and from VS Code, pressing
   go-to-definition twice against a deliberately slow server, before much is
   built on top of it.

   - *Zed* does send two requests

3. The shim caps concurrent heuristic queries and abstains past the cap. But a
   retry is itself a new query, so under load the second press - the one the
   whole retry protocol exists to serve - could be the one that gets dropped.
   Should retries of an already-pending spot bypass the cap, or hold reserved
   slots?

4. Should a slow-but-alive proper LSP be pre-empted, the way a warming one is?
   Right now it isn't - a server that has answered one definition request is
   treated as ready no matter how slowly it answers afterward. Doing otherwise
   reintroduces a "slow" health state, detected against the server's own rolling
   baseline rather than an absolute threshold. Whether that can be done without
   false positives needs measurements that don't exist yet, so the conservative
   version ships first.

5. Disk-file parse caches key on (path, mtime, len). Second-granularity mtime on
   some filesystems means a same-second rewrite of the same length serves a
   stale tree. Is a content hash worth the read, or is this rare enough to
   accept?

6. What should the shim do when the editor misbehaves - didOpen for a document
   already open, didChange for one never opened, didClose for one that isn't?
   `core.md` section 18.6 answers the part that affects
   correctness: mark the document untrusted, keep proxying, log it. What
   remains open is whether the user should be told, since silently ignoring
   hides editor bugs they would want reported.

7. Should the shim supervise and restart the proper LSP when it dies,
   instead of just exiting and letting the editor deal with it? The shim
   already holds authoritative text for every open document, so replaying
   state into a fresh server is nearly free, and it could serve heuristics
   through the whole gap. rust-analyzer restarts on Cargo.toml edits often
   enough that this might be the most noticeable feature. The cost is
   owning restart policy and backoff, which is real machinery.

8. How should multi-root workspaces order search scope? The folder containing
   the requesting document first is the obvious default, but a monorepo with
   many roots may want the pagerank-style ranking from question 1 instead.

9. Does the parse cache need a memory ceiling separate from its entry ceiling?
   Probably - one generated file can be enormous - but the right number depends
   on measurements that don't exist yet.

10. **Does standalone want the watcher that proxy mode defers?**
    `deps.md` section 7 defers `notify` because a stale file list
    costs a miss the proper LSP covers. In standalone it costs a permanent miss,
    so the case for watching is stronger here — possibly strong enough to make
    standalone the reason the watcher gets built.

11. **Error or `null` for abstention?** `core.md`
    section 17.5 picks the error on section 9's reasoning, but that reasoning was written
    about a transiently unresponsive server, where the failure really is
    transient. In standalone an abstention is permanent for that spot, and a
    permanent failure reported as a transient one is its own small lie. Needs a
    look at what Zed and VS Code actually render for each.

12. **Should `Point + Point` become `Point + PointDelta`?**
    `rope-modifications.md` gives the vendored rope's `LineIndex`,
    `ByteColumn`, and `Utf16Column` no arithmetic operators at all, on the
    grounds that adding two line numbers is meaningless. But `Point` and
    `PointUtf16` keep their `Add`/`Sub`/`AddAssign` impls, which do exactly
    that one level up - treating one operand as absolute and the other as
    relative. It is kept because rope's internals rely on it throughout, so
    removing it is a much larger change than the one that document describes.
    The fix is a distinct `PointDelta` type. Worth doing if position
    arithmetic turns out to be a source of bugs; not worth widening the
    vendoring patch for pre-emptively.

13. **How long may the candidate list be, and what happens at the cap?**
    The larger question this replaces - what to do when several candidates are
    equally plausible - is decided: return all of them, ranked. See "Several
    candidates" under Success metrics.

    What is left is the cap. A picker with six entries is an answer; one with
    sixty is worse than nothing, because the user pays the cost of reading it
    and then falls back to waiting for the LSP anyway. So there is a limit,
    and two things about it are unsettled.

    *Where it sits* needs a measurement rather than a guess - the useful
    number is how often the LSP's answer is at rank N, per stratum, which
    directly gives what a cap of N would cost in containment.

    *What happens when it is hit* is the more interesting half. Truncating to
    the best N keeps the answer and quietly drops the guarantee that
    containment was measuring, which makes the metric slightly dishonest in
    the one case where the tool knew least. Abstaining instead reintroduces a
    confidence-shaped decline, which is the thing the permissive posture was
    meant to remove - but it is defensible on different grounds, that "too
    many to be useful" is a statement about usefulness rather than about
    confidence. Truncation is the provisional choice, because it is the one
    that keeps producing data about the case.

14. **When the precision floor arrives, should it differ by mode?**
    `core.md` section 17.6 argues the counter-intuitive
    direction: *tighter* in standalone, not looser. A wrong answer in proxy
    mode is contradicted by the real LSP seconds later; in standalone there is
    no divergence report and it stands forever. Needs a measurement rather
    than an argument.

15. **Which server's behaviour should standalone imitate?** Now that the
    tool varies with the server behind it, standalone has no answer:
    there is nothing behind it. Either a neutral profile that makes no
    server-specific choice, or the most widely deployed server's profile
    on the grounds that it matches what users expect. The same question
    covers proxying a server we have no profile for. It matters more here
    than it looks, because there is no divergence report to correct a
    mismatched convention - see question 14.
