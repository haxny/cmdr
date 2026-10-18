# Land

`pnpm land` takes a worktree branch onto `main` through CI. Once the rulesets in `.github/rulesets/` are on, it's the
only way commits reach `main`: nobody, admin included, lands a SHA without a green `CI OK`.

## Module map

- `main.go`: the flow (push, PR, wait, land, sync), every step skipped when it's already done.
- `state.go`: the pure decisions (start step, CI verdict, messages, worktree parsing), tested in `state_test.go`.
- `exec.go`: git and gh plumbing. `rulesets.go`: `pnpm land rulesets [-apply]`.

## Usage

- `pnpm land`: push, open or reuse the PR, wait for `CI OK`, land, sync local `main`, delete the remote branch.
- `pnpm land -no-wait`: push and open the PR, then return. Run it between milestones to offload CI.
- `pnpm land -squash [-m MESSAGE]`: squash first (default message: every commit message, oldest first).
- `pnpm land -rebase`: rebase onto `origin/main` first when it moved.
- `pnpm land rulesets`: print what `-apply` would create or update on GitHub.

## Must-knows

- **It lands by pushing the green SHA: `git push origin <sha>:main`.** The admin bypasses the PR ruleset, the CI ruleset
  still demands `CI OK` on that exact SHA, SHAs stay identical, and GitHub marks the PR merged. Probe evidence:
  `DETAILS.md`.
- **Re-run it at any stage.** A red CI prints the failed jobs and `gh run rerun <id> --failed`; "main moved" prints
  `pnpm land -rebase`.
- **The landing push uses `--no-verify`.** CI passed on that exact SHA, and a pre-push hook that adds a commit would
  land something CI never saw.
- **Only a lead agent lands.** CI takes 25–35 min, longer than a foreground tool call, so agents run `pnpm land` in the
  background, and a background command never wakes a subagent. A subagent runs `-no-wait` and hands over.
- ❌ **Never commit on local `main`.** It mirrors `origin/main`; `land` fast-forwards it wherever it's checked out, and
  only when that tree is clean.

Rulesets, the probe, and decisions: `DETAILS.md`.
