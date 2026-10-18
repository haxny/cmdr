# Handling an outside contributor's pull request

Our own PRs are plumbing that `pnpm land` opens (`scripts/land/CLAUDE.md`), but people send real ones. The goal is that
a contributor does nothing beyond opening one: no rebase request, no formatting fixups, no "please squash". `main` moves
several times an hour, so anything we ask someone to redo is stale before they read the message.

## The flow

1. **Take the code into a worktree.** `~/.claude/scripts/new-worktree.sh pr<N>-<slug>`, then
   `git fetch origin pull/<N>/head` and `git cherry-pick FETCH_HEAD`. A conflict in a generated file (`bindings.ts`) is
   normal: resolve it and let `bindings-fresh` confirm.
2. **Fix whatever CI would reject, ourselves.** An outside tree is usually not `rustfmt` / `oxfmt` clean, and new
   must-knows can push a `CLAUDE.md` past its word budget. Land those as separate `style:` / `docs:` commits so the
   contributor's own commit stays theirs.
3. **Leave their commit message alone.** It feeds the release notes, but our conventional-commits rule is for us, and
   rewriting someone's words to satisfy it buys nothing.
4. **Record the PR head** (below), then `pnpm land`.
5. **Comment and thank them**, naming what landed and what we changed on top.

## Recording the PR head so GitHub marks it merged

GitHub marks a PR merged only when its head SHA becomes an ancestor of the base branch, or when the merge runs through
GitHub's own API. Cherry-picking rewrites SHAs, so without this the PR stays open forever. An `ours` merge on the
worktree branch makes the head an ancestor while leaving the tree byte-identical:

```sh
git fetch origin pull/<N>/head:refs/tmp/pr<N>
git merge -s ours --no-ff refs/tmp/pr<N> -m "chore: record PR #<N> as merged"
```

Check before landing: `git diff HEAD^1` prints nothing, and `git merge-base --is-ancestor refs/tmp/pr<N> HEAD` succeeds.
GitHub closes the PR with the Merged badge when `land` puts it on `main`.

- **Gotcha / Why:** the merge commit's diff against its first parent is empty, which reads like a mistake. That's what
  it's for: it carries only the ancestry. Don't "clean it up".
- **Gotcha / Why:** a plain rebase flattens merges, so `pnpm land -rebase` and `-squash` drop it. If `main` moved, run
  `git rebase --rebase-merges origin/main`, then `pnpm land`.

We take these merge commits in an otherwise linear history on purpose: the Merged badge is part of what a contributor
gets for the work, and a gray "Closed" reads as a rejection.
