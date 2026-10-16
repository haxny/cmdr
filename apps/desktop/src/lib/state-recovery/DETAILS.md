# State recovery notices: details

Issue #461. Must-knows in `CLAUDE.md`; the backend flow in `apps/desktop/src-tauri/src/state_recovery/DETAILS.md`.

## Decisions

- **A persistent warning, not an error.** Nothing is broken now: Cmdr kept a copy and carries on. But the person lost
  something (a list, a history, a choice), so it stays until they close it rather than timing out.
- **The path is shown as text and offered in Finder.** The acceptance bar is "names where the copy is"; text alone would
  make the person dig through `~/Library`.
- **No toast group.** A group of persistent toasts refuses newcomers once full, which would drop a recovery silently.
  Several at once means a disk in trouble, and each one matters then.
- **No report offer.** A damaged state file is the disk's or a crash's story, not a bug report; the security log and the
  kept copy hold what David would need.
- **Copy**: one headline per file saying what reset and how ("so the list starts fresh"), no "error" or "failed"
  (`docs/style-guide.md`). Only the settings and license cases carry a body, because only there is the next step not
  obvious. Drafts, for David's review.
