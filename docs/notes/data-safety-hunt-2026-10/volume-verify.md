# Data-safety hunt 2026-10: volume verifier report (`volume`)

Verifier for `volume-hunt.md`. Read-only, static tracing (no app run).

### volume-1

- **Verdict:** confirmed (traced end to end statically, including the frontend per-row gate the hunter didn't trace).
- **Severity:** medium (agree). Loss is recoverable (journaled rename, undo, or a manual rename back) until a later
  `git gc` past `gc.pruneExpire` can drop objects that only a linked worktree's index or detached HEAD kept alive.
- **Excerpts:** verbatim and current: `crates/cmdr-operations/src/rename.rs:148-156`,
  `crates/cmdr-git/src/virtual_listing.rs:40-48` (the row path is `<repo>/.git/worktrees`, the real dir's path).
- **Shadowing is real and intended:** `apps/desktop/src-tauri/src/file_system/git/DETAILS.md` § Decisions ("A real entry
  whose name collides with a virtual category gives way to it ... `.git/worktrees/` in a linked-worktree setup").
- **F2 / click-to-rename reach it, no per-row gate anywhere on the FE:**
  - `apps/desktop/src/lib/file-explorer/pane/file-operation-commands.ts:113-155` `readOnlyRefusal` judges
    `capabilitiesForPane(volId, panePath)` only. Pane path `<repo>/.git` doesn't match `isVirtualGitPath`
    (`git/path-detection.ts:19`, needs a category segment after `.git/`), so the pane gets the drive's writable kind.
  - `pane/rename-flow.svelte.ts:536-541` `tryActivate` refuses only a missing entry or `..`; `activateRename` (178-230)
    runs `checkPermission` on the real path, which passes.
  - `rename/rename-operations.ts:111,135,149` → `checkRenameValidity(<repo>/.git, "worktrees", new)` (plain local stat),
    then `renameFile(<repo>/.git/worktrees, <repo>/.git/<new>, volumeId)`.
  - `commands/rename.rs:210-243` `rename_file_replying` → `rename_managed` with no portal check; `rename_managed_inner`
    only diverts archive paths, and `crates/cmdr-operations/src/rename.rs` has no `portal` reference at all.
    `check_rename_permission_for_volume` (`rename.rs:597`) has none either.
- **Multi-rename (⌃M) reaches it too:** `refuseMultiRename` (`file-operation-commands.ts:176-186`) is pane-level;
  `multi_rename/run.rs` `apply` refuses only when `resolve(volume_id, dir).routed` is set, and `dir` is `<repo>/.git`
  (not routed), then `SourceFingerprint::capture_local(dir.join("worktrees"))` succeeds on the real dir and
  `start_renames` renames it (`rename/bulk.rs:126`, root lane, raw local paths).
- **MCP `rename`:** the non-autoConfirm path dispatches `fileRenameCommand` (`routes/(main)/mcp-listeners.ts:625-653`),
  same as F2. The `autoConfirm: true` path skips the FE entirely: `mcp/executor/file_ops.rs:320-360` resolves the row
  off pane state and calls `rename_file_replying` directly, so it doesn't even get the pane-level gate (see volume-v1).
- **Agent planner is NOT a route:** `agent/tools/propose/rename/plan.rs:385,398` refuses folders and `portal_serves`
  paths, so Ask Cmdr can't propose this.
- **Related existing issue:** #463 (open) tracks the missing backend gate for bulk rename, numbered create, and
  rename-as-move, but assumes "the frontend's refusal to write in a routed pane" holds
  (`crates/cmdr-operations/src/host/DETAILS.md` ~line 82). volume-1 shows that assumption fails for the shadowed `.git/`
  rows, and the single-item `rename_managed_inner` isn't in #463's list. Not a duplicate; worth folding into #463's fix.

### volume-v1 (new, verifier-found). MCP `rename` with `autoConfirm` renames a linked worktree's real admin dir from inside the read-only `.git/worktrees` portal pane

- **Verdict:** plausible (each hop read; not run). **Severity:** medium (same impact class as volume-1, one worktree).
- **Mechanism:** inside a `.git/worktrees/` portal pane (F2 there IS refused by `readOnlyRefusal`), each row's path is
  `<repo>/.git/worktrees/<id>` where `<id>` is gix's worktree id (`crates/cmdr-git/src/virtual_listing.rs:440-460`),
  which is exactly the name of git's real admin dir. The pane keeps the parent drive's `volumeId` (`pane/CLAUDE.md`). An
  AI client calling `rename {name: <id>, newName: X, autoConfirm: true}` hits `mcp/executor/file_ops.rs:335-357`: no
  capability check, only `refuse_while_dialog_blocks`, then `rename_file_replying` → `rename_managed_inner`, root volume
  → `rename_no_replace(<repo>/.git/worktrees/<id>, <repo>/.git/worktrees/X)` on the real dir.
- **Also a parity break:** `mcp/CLAUDE.md` says agents do only what users can do; the user can't F2 there.
- **Fix:** the same backend `portal_serves(from) || portal_serves(to)` refusal in `rename_managed_inner` closes both
  volume-1 and this one; the MCP autoConfirm path should also refuse on the pane's capability kind.

### Near misses

Skimmed all 12; none looked wrong enough to raise. The `.cmdr-tmp-` reaper's substring match is the only one with a real
(hygiene-level) edge, and it's the transfer engine's slice.
