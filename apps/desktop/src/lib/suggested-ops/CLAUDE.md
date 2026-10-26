# AI suggestions (`lib/suggested-ops/`)

The review surface for what Ask Cmdr proposed (titled "AI suggestions"; code says "suggested ops"): a soft dialog
listing pending sweeps and the groups inside them, every group's files shown at once, with per-group approve and reject
and per-op deselection. Backend: `apps/desktop/src-tauri/src/commands/agent/suggested_ops.rs` over the proposal spine
(`apps/desktop/src-tauri/src/agent/store/proposals/CLAUDE.md`). Depth: `DETAILS.md`.

## Module map

- `suggested-ops-trigger.svelte.ts`: the state: sweeps, one `GroupReview` per group (window, shown rows, deselection,
  live facts, change notice), approve, reject.
- `SuggestedOpsDialog.svelte` → `SuggestedOpsGroup.svelte` (header, pills, facts, reason, actions) →
  `SuggestedOpFiles.svelte` (the `ColumnList`, Show more / Show all). `AgentReason.svelte`: the labelled AI words.
- `suggested-ops-copy.ts`: pure verb → button / undo-reason keys, and a row's live-vs-snapshot facts.
- `suggested-ops-badge.svelte.ts` / `SuggestedOpsIndicator.svelte`: the status-corner count and its subscription.
- `suggested-ops-failure.ts` / `suggested-ops-refusal.ts`: the typed failure, and a refused approval's reason.

## Must-knows

- **The layout is the argument.** The AI's reason renders ONLY through `AgentReason`, under a label naming it as the
  AI's words, beside facts Cmdr holds itself (folders, destination, sizes). ❌ Never render a rationale unlabelled: it
  reads as something Cmdr checked.
- **Size and Last modified are LIVE**, read for drawn rows only. A value that moved shows a warning glyph with both; a
  gone file says so; an unchecked one falls back to the snapshot WITH a label. ❌ Never a zero for an absent value, ❌
  never an unlabelled snapshot.
- **Reversibility and "the folder will be created" are DISCLOSED, never blocking.** The pill says WHY, by verb
  (`reversibilityKey`); a trash on a drive with no Trash is irreversible and says so.
- **Each group loads a WINDOW at a time** (`OP_WINDOW_SIZE`), sized by `COUNT(*)`, through `ColumnList`'s windowed
  source. A group of 60,000 is legitimate: ❌ nothing loads a group to show it, "Show all" included.
- **Deselection is a per-group set of op ids, with deliberately no "deselect all".** Approving sends the ids turned OFF,
  so "all" is the empty set; Reject already says "none".
- **A group that changed under the review is ANNOUNCED, never swapped**, and its reload keeps the deselection: dropping
  it would quietly turn a "no" back into a "yes".
- **A change notice keys on `reason`, not on the group id alone.** An approval and an amendment carry the same id.
- **The badge is its own module, seeded once AND subscribed, never polled.** ❌ Don't drop the seed: suggestions never
  expire.
- **A refused approval gives the group back with its reason under it.** ❌ Never word an engine refusal with new copy.

Flows, decisions, and the gallery preview: `DETAILS.md`.
