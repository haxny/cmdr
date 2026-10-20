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
and a branch already on `origin/main` skips to syncing `main` and deleting the remote branch. The pre-push hook may
commit a formatting fix and stop that push; `pushBranch` then pushes once more with the fix on top. Untracked files
don't count as dirty: they can't change what lands, and the main clone, where `release.sh` lands its release commit
from, often has some.

`scripts/release.sh` depends on this interface: `pnpm land` from the main clone, exit 0 only when the SHA is on
`origin/main`, local `main` synced by `fetch origin main:main`, and any CI run on the SHA counting. Change it with the
release flow (`docs/guides/releasing.md`).

`ciVerdict` takes the newest CI run on the SHA (a `gh run rerun` reopens it, so an in-flight re-run reads as pending,
not red) and the newest `CI OK` check run. Only `success` is green: GitHub would accept a skipped check, which a
finished run should never produce.

## Gotcha: the branch push must never overwrite commits only origin has

Gotcha: a `--force-with-lease` whose expected SHA is read (`ls-remote`) right before the push protects nothing: it
always matches, so origin's branch gets overwritten whatever it holds. It clobbered GitHub's "Update branch" merge on PR
#481 (2026-10-20). Why the current shape (`pushBranch`, `pushToOrigin`):

- Overwriting origin's branch is legitimate only when this branch already integrated what's there: a local rebase or
  `-squash` of commits it pushed earlier.
- `fetchBranch` updates `refs/remotes/origin/<branch>` (or deletes it when origin has no such branch, as after an
  earlier land). If that tip is neither an ancestor of `HEAD` nor in the branch's reflog, `pushBranch` stops with
  `errOriginAhead` and tells the user to `git pull --rebase` or `git reset --hard origin/<branch>`. Deciding this up
  front keeps the refusal typed, with no stderr matching.
- The push itself uses a bare `--force-with-lease=refs/heads/<branch>` (checks origin against the tracking ref) plus
  `--force-if-includes` (refuses a tracking tip the local branch never had), the backstop for a push landing between the
  fetch and ours. Git ignores `--force-if-includes` next to an explicit lease value, so never add one back.
- Tested against real bare origins in `push_test.go` (verified on git 2.56.0; removing `--force-if-includes` turns
  `TestPushToOriginRefusesAFetchedButUnintegratedTip` red, 2026-10-20).

## Decision: cheap polling that outlasts the API budget

Many sessions landing at once poll one account, and on 2026-10-20 they spent its 5,000/h REST budget: `gh run list`
answered 403 after CI was green, and that land was lost. So:

- **One REST call per poll** (`repos/{repo}/actions/workflows/ci.yml/runs?head_sha=`), plus the `CI OK` check runs only
  once the run finished. `gh run list --workflow` cost two calls (it resolves the workflow first).
- **Poll by run age** (`pollDelay`): 20 s until the run shows up, every 3 min while it's younger than 15 min (CI takes
  25–35 min), then every minute, each plus up to 25% jitter. About 15–25 calls per CI run, down from ~180 (three calls
  every 30 s).
- **A failed call waits out a spent budget** (`afterGHFailure`, `ghbudget.go`), then continues: it sleeps until the
  reset, or a secondary limit's `Retry-After`. Reads otherwise get two short retries; writes (`gh pr create`, ruleset
  apply) get none, since they may have gone through.
- **The budget comes from the failing response's own headers** (`gh api --include`, parsed by `net/http`, so no stderr
  matching). Gotcha: `gh api rate_limit` can disagree with them (2026-10-20: `core` showed 2,862 left while the Actions
  API answered `X-RateLimit-Remaining: 0`), so it's only the fallback for `gh pr` / `gh run` calls, which print no
  headers.

## Decision: rebase by default, capped

Decision: when `origin/main` moved, `land` rebases onto it before pushing, and when it moves while CI runs, it rebases
and waits for CI again, up to `maxCIRuns` (8) runs. Why: CI runs on the rebased SHA anyway, so a textually clean rebase
that breaks something still can't land, and the old "rerun with `-rebase`" step only cost an agent a round trip. Eight,
because several sessions landing at once each restart the others' CI, and a land that gives up after three hands the
wait back to a person for nothing. A re-run is warmer than the first (`rust-cache` restores `target/`; Desktop (Rust)
went from 34 to 18 min on PR #497's second run, 2026-10-19). The cap stays so a `main` that never settles can't spin a
land for a whole day without saying so.

Exception: a branch carrying a merge commit (a contributor's PR record, `docs/guides/handling-prs.md`; a release
merge-back, `docs/guides/releasing.md`) is never rebased, since a plain rebase flattens the merge. `land` stops and
names the manual fix instead.

## Open points

- **Renovate automerge**: Renovate uses GitHub's native auto-merge (`platformAutomerge`). Whether GitHub honors an app's
  bypass for auto-merge is unverified; if Renovate PRs stall on the approval rule, set `platformAutomerge: false` so
  Renovate merges through the API itself.
