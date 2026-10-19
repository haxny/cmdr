# Data-safety hunt 2026-10: hunter and verifier brief

Second adversarial data-safety hunt (GitHub issue #147). The first one (2026-09-01) covered only the copy/move transfer
engines (`crates/cmdr-operations/src/transfer/`) and found 15 real bugs. This one covers everything else. Each slice of
the code gets one hunter, then one verifier.

## Severity scale

- **critical**: silent permanent loss of user data in a realistic flow.
- **high**: loss or corruption needing an unusual but plausible condition, or loss the user is told about but can't
  recover.
- **medium**: wrong result, wedged state, or recoverable loss.
- **low**: hygiene.

"User data" means the user's files, folders, archives, and remote content, plus their settings, secrets (saved
credentials, license), favorites, tags, history (operation log), and anything else they'd be upset to lose. A read-only
subsystem can still lose Cmdr's own state (an index, a cache); that counts only as medium at most unless it leads to a
write that harms user data (for example, a stale index driving a delete).

Also in scope: security issues that put user data at risk (path escapes like zip-slip, `..` in remote names, symlink
following out of a selected tree, secrets written to logs or world-readable files).

## Hunter rules

- **Read-only on code.** Don't edit any source file, don't build, don't run the app. Reading, `codegraph_*`, `rg`, and
  `git log`/`git blame` are fine. Run commands in the foreground only.
- Read every non-test source file in your slice in full (use the slice's `CLAUDE.md`/`DETAILS.md` first for the map).
  Follow calls out of the slice when a mechanism depends on them.
- Don't re-report the transfer engines' copy/move logic (covered by the first hunt) unless your slice's code reaches a
  NEW hole through it.
- **Kill each candidate before reporting it**: look for the guard (a check upstream, a lock, a no-replace primitive, a
  test that pins the behavior). Report only what survives. Quality over count: at most eight findings, ranked.
- Write your report to `docs/notes/data-safety-hunt-2026-10/<slug>-hunt.md` (slug given in your prompt), in this schema
  per finding:

```
### <slug>-<n>. <one-line title saying what goes wrong for the user>

- **Severity:** critical | high | medium | low
- **Confidence:** confirmed (traced end to end) | plausible (one step not traced)
- **Where:** `path/to/file.rs:LINE`, `fn_name` (plus any other cited sites)
- **Excerpt:** a verbatim code excerpt (5–25 lines) showing the defect, in a fenced block with its start line
- **Mechanism:** the call-ordered chain that leads to the loss
- **Trigger:** what a real user does (or what happens around them) to hit it
- **Guard ruled out:** which protections you looked for and why they don't apply
- **Impact:** what is lost or corrupted, and whether it's recoverable
- **Suggested fix:** the shape of a fix, one or two sentences
```

- End the file with a `## Covered` section listing every file you read in full, and `## Not covered` for anything in
  your slice you didn't get to. Also `## Near misses` (one line each) for candidates you killed and the guard that
  killed them; this helps the verifier and future hunts.
- If you find nothing that survives, say so plainly; an empty slice is a valid result.
- Final message: just the file path, the finding count by severity, and one line per finding title. Keep it under 1,500
  characters.

## Verifier rules

- **Read-only on code**, foreground only, same as the hunter.
- For each finding in `<slug>-hunt.md`: open the cited lines, check the excerpt is verbatim and current, trace the
  mechanism yourself, and actively try to refute it (look for a guard the hunter missed, an upstream check, a caller
  that can't produce the input, a test proving otherwise).
- Write `docs/notes/data-safety-hunt-2026-10/<slug>-verify.md` with, per finding: `### <slug>-<n>`, a verdict
  (**confirmed** / **plausible** / **refuted** / **duplicate of <id>**), the severity you'd assign (may differ from the
  hunter's, say why), and a few lines of evidence with `file:line`. For a refuted one, name the guard.
- Final message: the file path and one line per finding, `<id>: <verdict>, <severity>`. Under 1,000 characters.
