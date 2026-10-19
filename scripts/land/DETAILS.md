# Land: details

Must-knows and usage: `CLAUDE.md`. Why `main` is gated at all: GitHub issue #475.

## The rulesets

The JSON in `.github/rulesets/` is the source of truth; `pnpm land rulesets -apply` creates or updates each one on
GitHub by name and leaves every other ruleset alone.

- `main-pr-required.json` ("main: PR with approval"): a PR with one approval, stale approvals dismissed, last-push
  approval. Bypass: the repository admin role (`RepositoryRole` 5, David) and the Renovate app (`Integration` 2740, the
  `renovate` app that opens and merges our Renovate PRs). These are the settings Scorecard's Branch-Protection tier 2
  looks for; the bypass actors cost only tier 5.
- `main-ci-required.json` ("main: CI OK required"): the `CI OK` check (job `ci-ok` in `.github/workflows/ci.yml`),
  pinned to the GitHub Actions app (`integration_id` 15368) so no other app can post a passing `CI OK`. `strict` is on
  for Scorecard's "up to date before merge"; it only affects PR merges, since a direct push to `main` must fast-forward
  and so always contains `main`. **No bypass actors**: nobody lands red. Releases need none either: the pipeline pushes
  nothing to `main`, and `release.sh` lands its commit through `pnpm land` (`docs/guides/releasing.md` § What a release
  publishes).
- Two older ones stay as they are: "main: no force-push or deletion" and "Release tags (v*)".

Turning off the two `main` rulesets restores direct pushes, and `land` keeps working without them (it pushes a green SHA
either way).

## Decision: land by pushing the green SHA

Decision: after `CI OK` goes green on the PR head, `git push --no-verify origin <sha>:refs/heads/main`. Why: it
fast-forwards, so SHAs, signatures, tags, and local `main` stay identical to what CI tested, and GitHub still marks the
PR merged. The alternative, `gh pr merge --admin --rebase`, rewrites every SHA.

Probe (2026-10-18, temporary rulesets mimicking A and B on `refs/heads/probe-land/main`, a throwaway workflow posting
`probe-ok`; all of it deleted afterwards):

- A SHA without the check, pushed by the admin: rejected, `GH013 ... Required status check "probe-ok" is expected`. So
  the CI ruleset holds against the admin.
- The same SHA once `probe-ok` was green on its PR: accepted, with
  `Bypassed rule violations: New changes require approval from someone other than the last pusher`. The PR (#477) turned
  `MERGED`, `mergeCommit` = the head SHA, `mergedBy` = vdavid.
- `github-actions[bot]` pushing with `GITHUB_TOKEN`: rejected by both rules. The GitHub Actions app can't be a bypass
  actor on this user-owned repo: the API answers 422 "Actor GitHub Actions integration must be part of the ruleset
  source or owner organization". A `DeployKey` bypass actor and the Renovate app are both accepted.

## Decision: the PR is plumbing

`land` opens the PR on the first push, because CI runs on `pull_request`, so every later push re-runs CI with no extra
trigger. Title: the oldest commit's subject. Body: every commit message. Nobody reads it; the commit messages are the
record. A closed PR is never revived: `land` opens a fresh one.

## Decision: idempotent, no state of its own

Each run reads the branch and GitHub and picks up from there (`nextStartStep`, `ciVerdict`): an already-pushed branch
isn't pushed again, an open PR is reused, a `-squash` re-run on one commit is a no-op (a fresh SHA would restart CI),
and a branch already on `origin/main` skips to syncing `main` and deleting the remote branch. The branch push uses
`--force-with-lease` with the SHA it just read, so a push from another machine in between is refused, not overwritten.
The pre-push hook may commit a formatting fix and stop that push; `pushBranch` then pushes once more with the fix on
top. Untracked files don't count as dirty: they can't change what lands, and the main clone, where `release.sh` lands
its release commit from, often has some.

`scripts/release.sh` depends on this interface: `pnpm land` from the main clone, exit 0 only when the SHA is on
`origin/main`, local `main` synced by `fetch origin main:main`, and any CI run on the SHA counting. Change it with the
release flow (`docs/guides/releasing.md`).

`ciVerdict` takes the newest CI run on the SHA (a `gh run rerun` reopens it, so an in-flight re-run reads as pending,
not red) and the newest `CI OK` check run. Only `success` is green: GitHub would accept a skipped check, which a
finished run should never produce.

## Decision: rebase by default, capped

Decision: when `origin/main` moved, `land` rebases onto it before pushing, and when it moves while CI runs, it rebases
and waits for CI again, up to `maxCIRuns` (3) runs. Why: CI runs on the rebased SHA anyway, so a textually clean rebase
that breaks something still can't land, and the old "rerun with `-rebase`" step only cost an agent a round trip. The cap
is there because each run takes 25–35 min: on a `main` busy enough to move under three runs in a row, an unbounded loop
would spin for hours without saying so.

Exception: a branch carrying a merge commit (a contributor's PR record, `docs/guides/handling-prs.md`; a release
merge-back, `docs/guides/releasing.md`) is never rebased, since a plain rebase flattens the merge. `land` stops and
names the manual fix instead.

## Open points

- **Renovate automerge**: Renovate uses GitHub's native auto-merge (`platformAutomerge`). Whether GitHub honors an app's
  bypass for auto-merge is unverified; if Renovate PRs stall on the approval rule, set `platformAutomerge: false` so
  Renovate merges through the API itself.
