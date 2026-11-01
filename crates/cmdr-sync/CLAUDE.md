# cmdr-sync

Synchronize folders, the pure part: what a volume can do, how names pair up across volumes, and the one answer to "is
this copy newer?". No `tauri`, no I/O, depends on `cmdr-fs` only.

## Module map

- **caps.rs**: `SyncCaps::of(&dyn Volume)` and `backend_facts(BackendKind)` (case sensitivity, mtime precision).
- **key.rs**: `NameRule` (`FoldCase` / `ExactCase`, NFC always) and `EntryKey`, the path of name keys.
- **compare.rs**: `EntryFacts`, `CompareRules`, `classify`. Tests sit beside each file as `*_test.rs`.

## Must-knows

- ❌ **`classify` is shared with ⇧F2** (`cmdr-listing`'s compare). A change here moves Compare folders too; run
  `cargo nextest run -p cmdr-listing compare` with it.
- **`classify` never yields `NameClash` or `Unreadable`.** The planner sets those.
- **A missing time never claims "newer"**: sizes decide. Only a difference we can see counts.
- **Names pair by key, operations use each side's own spelling.** `NameRule::key` is for matching, ❌ never for a path
  handed to a volume.
- **The crate never stats a child and never deletes permanently.** Phase 1 has no delete step at all.
- **No `cmdr-operations` dependency**: `cmdr-listing` depends on this crate and the operations test build depends on
  `cmdr-listing`, so a link back would be a cycle. The executor will sit behind a trait.
- No `tauri`, enforced by `index-crate-isolation`.

The classify table, the per-backend facts with evidence, and the dependency decision: `DETAILS.md`.
