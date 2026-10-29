# Trust page review

getcmdr.com/trust (with `/trust/development` and the accessibility pages) tells security and IT reviewers what Cmdr
sends, stores, and promises. Every claim on it must be true of the RELEASED app, and a reviewer can check it against the
source, so an overstatement costs more than a gap. `verifiedAgainst` in `apps/website/src/lib/trust.ts` names the
release the page was last checked against, and the page shows it.

Each release reviews only what changed since that stamp, then moves the stamp to the new version. `scripts/release.sh`
refuses to release while the stamp names another version.

❗ The website deploys from `main`, so a trust-page edit goes live the moment it lands, before the code it describes
ships. A claim about unreleased code waits for the release prep (the edit rides the release commit). Landing it early
overstated the released app once: "12 languages" went live while 0.57.0 shipped 11.

## Why it works this way

- **The stamp drifted six releases** (0.51.0 to 0.57.0, 2026-10-08 to 2026-10-29) with nothing noticing, while the
  security event log, the CRA fixes, and more shipped. A stale stamp tells a careful reviewer the page isn't maintained,
  which costs more trust than any single wrong line.
- **Rereading the whole page every release is too expensive to keep doing**, so the source map narrows each review to
  the sections whose code moved.

## The pieces

- **The source map**, `apps/website/src/lib/trust-sources.json`: for every `<h2 id>` of the trust pages, the repo paths
  whose changes could make that section's claims wrong (patterns: `path.Match` per segment, plus `**`). When you add a
  claim that rests on code the section's paths don't cover, add the path. When you add an `<h2 id>`, add its entry.
- **The tool**, `./scripts/trust-review.sh` (`scripts/trust-review/`): with no arguments, it diffs from the stamp's tag
  to HEAD and prints each section whose paths changed, with the commits that touched them. `-since` and `-to` take any
  refs. `validate` checks the map against the pages.
- **The check**, `website-trust-sources`, runs `validate`: it fails on a heading with no entry, an entry whose heading
  is gone, or a pattern that matches no tracked file (a moved module). It runs in CI's repo-wide job, since a rename
  anywhere can break it.
- **The gate** in `scripts/release.sh`: the release stops unless `verifiedAgainst.version` equals the version being
  released. The trust files are release-prep files, so the review's edits ride the release commit like the changelog.

## The review, per release

It's part of step 6 of `docs/guides/releasing.md` § The release procedure, once David has picked the version. A
security-only patch release does it too (§ Security-only patch release, step 5); its diff is tiny, so it takes minutes.

1. **Run `./scripts/trust-review.sh`** in the main clone after syncing `main`. Its list is the work. A section the tool
   skips can still go stale through a product decision with no code (a new subprocessor, a changed price, a closed gap):
   skim the CHANGELOG draft for those.
2. **Check each listed section's claims** against the code being released and the commits shown. Fix what's wrong in the
   page sources (`trust.ts`, `trust-mdm.ts`, `trust-development.ts`, `trust.astro`, `trust/*.astro`). Phrasing follows
   `docs/style-guide.md`; David reviews the diff with the rest of the release prep.
3. **Look for what's missing**, not only what's wrong: a new outbound host, a new stored file, a new MDM key, a closed
   known limitation that still sits in `knownLimitations` (that undersells us).
4. **Update the map** where a claim now rests on new code, and run `pnpm check website-trust-sources`.
5. **Stamp it**: set `verifiedAgainst` to the new version and today's date.

Sizing: a typical release touches a few sections and takes one agent well under an hour. A big gap (many releases, or a
large feature like a new backend) is worth one subagent per group of sections, each writing its findings to a file; the
first full review (2026-10-29) used five.

## Not covered by the per-release pass

- **Measured numbers** on `/trust/development` (`trust-development.ts`: release cadence, update adoption, advisory fix
  times) carry their own `measuredOn` window and refresh commands. Refresh them together, about once a quarter, not per
  release.
- **The accessibility conformance report** (`/trust/accessibility`) is a formal evaluation. The review flags claims the
  code clearly contradicts; a re-evaluation is its own task.
- **External facts** (a subprocessor's certifications, DPF status) don't show up in a diff. The yearly security review
  rechecks them (vault `projects/Cmdr/business/Cmdr yearly security review.md`).
