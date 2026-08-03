1. When it's whole project search, how to choose the better module when
   it's heuristic? Maybe something like repomap's pagerank?

2. **Does the editor actually send a second go-to-definition request while the
   first is still pending?**
   **Resolved — void, there is no retry protocol.** `shim.md` §7 removes
   repeat detection entirely: `Spot`, `is_repeat_of`, and the re-anchoring of
   pending positions through every `didChange` are gone, and the policy table
   is now eager while the server is `Warming` or `Unresponsive` and silent
   while it is `Ready`. Nothing depends on what the editor does with a second
   press, so nothing needs the trace.

   Recorded rather than deleted, because the question is the reason the
   machinery went. It was answered for Zed (two requests) and never for VS
   Code, so half the target audience might never have triggered it — and what
   it served was one narrow case, a `Ready` server slow on one query, which
   `high-level.md` already described as the shim answering "almost nothing".
   Intricate state, unverified premise, thin payoff. If the slow-but-alive
   case is ever worth serving, question 4 is the cheaper place to serve it,
   because health is inferred from the server's own behaviour rather than from
   the user's typing habits.

3. **Should retries bypass the in-flight cap?**
   **Resolved — void, same reason.** There are no retries to bypass it. The
   observation that made this worth asking survives as evidence in question 2:
   under load the cap could drop the second press, the very one the protocol
   existed to serve.

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
   `core.md` section 8.6 answers the part that affects
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

10. **Does standalone want a watcher of its own?** Narrowed, and now a
    question about one mode rather than about the tool.

    **In proxy mode it is settled, and the answer is no.** The child registers
    file watching with the editor, and the editor's
    `workspace/didChangeWatchedFiles` notifications pass through the shim on
    their way to it, so the file list is invalidated from a signal already on
    the wire — `core.md` section 4 and `shim.md` section 3. A `notify` watcher
    there would duplicate it, and pay descriptors and its own exclusion rules
    to do so.

    **In standalone there is no editor doing this**, and no proper LSP to
    cover a miss, so a stale list costs a permanent one. That is the only
    remaining case for `notify` (`deps.md` section 7) and possibly the reason
    it eventually gets built.

    What still argues against it there: the `NoCandidates` rescan repairs the
    list on the next query at that spot, cheaply and with no dependency. So
    the real question is how often a standalone user asks for a definition
    that was created seconds ago and does not press again — which is
    unmeasured, and is the measurement to take before adding the crate.

11. **Error or `null` for abstention?** `shim.md`
    section 14.5 picks the error on `core.md` section 5's reasoning, but that reasoning was written
    about a transiently unresponsive server, where the failure really is
    transient. In standalone an abstention is permanent for that spot, and a
    permanent failure reported as a transient one is its own small lie. Needs a
    look at what Zed and VS Code actually render for each.

12. **How long may the candidate list be, and what happens at the cap?**
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

13. **When the precision floor arrives, should it differ by mode?**
    `shim.md` section 14.6 argues the counter-intuitive
    direction: *tighter* in standalone, not looser. A wrong answer in proxy
    mode is contradicted by the real LSP seconds later; in standalone there is
    no divergence report and it stands forever. Needs a measurement rather
    than an argument.

14. **Which server's behaviour should standalone imitate?** Now that the
    tool varies with the server behind it, standalone has no answer:
    there is nothing behind it. Either a neutral profile that makes no
    server-specific choice, or the most widely deployed server's profile
    on the grounds that it matches what users expect. The same question
    covers proxying a server we have no profile for. It matters more here
    than it looks, because there is no divergence report to correct a
    mismatched convention - see question 13.

15. **How is a language with no usable server measured at all?**
    `high-level.md` ranks "languages with no usable server" *first* among the
    reasons standalone exists — there the heuristic is not a stopgap for
    something better, it is the only thing on offer. But every number in the
    design is defined against a language server's answer: `data-collection.md`
    collects truth by driving one, `core.md` §7 makes the proxied server the
    definition of correct, and standalone reports no divergence at all
    (`shim.md` §14.4). So the use case ranked first is the one nothing in the
    plan can score, and the seven languages chosen in phase 1b all have
    servers — partly because the pipeline requires it.

    **The likely answer is an LLM as the oracle**: given the file, the
    position, and the repository, ask a model where the definition is, and
    freeze its answers into a `truth.jsonl` of the same shape. That fits the
    existing machinery better than it first appears — the provenance header
    names a model and a prompt hash instead of a server and a version, and
    everything downstream (positions enumerated once, replay, the agreement
    predicate, the per-stratum table) is unchanged, because they only ever
    consume a frozen list of answers.

    What is genuinely unsettled:

    - *Is it trustworthy enough to be an oracle?* Answerable directly, and
      cheaply, before it is relied on: run it on Rust, where rust-analyzer's
      answer is already recorded, and report the LLM's agreement rate against
      it per stratum. That number is the whole decision. It should be
      collected during phase 1.5, when the comparison costs nothing extra.

    - *Its errors are correlated with ours, which a language server's are
      not.* A model reading a file with no type information is guessing from
      names and imports, which is what the heuristic does — so it will tend to
      be wrong in the same places, and the metric will flatter us exactly
      where we are weakest. That is a different failure from a noisy oracle
      and is not fixed by more samples. The per-stratum agreement rate above
      is also what would expose it.

    - *May an LLM-derived row ever share a table with a server-derived one?*
      Probably not. `core.md` §7 already refuses to average across two
      *servers* on the grounds that the mix is not a fact about the tool, and
      the gap between a server and a model is larger than the gap between two
      servers.

    - *How much can it cover?* 20k positions per repository
      (`data-collection.md` §3) is a real token bill, and it is the one place
      in this project where corpus size trades against money rather than
      against machine hours. A smaller sample with wider intervals may be the
      right answer for these languages.

    - *What counts as correct when there is no server to stand in for?* This
      is `resolution.md` open question 17 arriving from the other direction,
      and the two should be answered together: an LLM oracle does not just
      supply missing data, it silently *defines* the convention the handler
      will be tuned toward.

    Until this is answered, note what the current plan implies: standalone's
    first-ranked use case is served on the assumption that resolution quality
    tuned against languages that do have servers transfers to languages that
    do not. That may well be true. It is unmeasured, and it should be stated
    as a bet rather than left to look like coverage.

16. This can offer something that most LSPs do not - lookups from places like
    comments. Possibly also greater support for cross-language lookups (and from
    markdown docs etc)
