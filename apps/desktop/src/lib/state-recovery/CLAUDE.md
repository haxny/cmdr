# State recovery notices (frontend)

The toast that tells the person Cmdr couldn't read one of its own state files and started it fresh: which state reset,
and where the old copy went (#461). The backend sets the file aside and queues the notice:
`apps/desktop/src-tauri/src/state_recovery/CLAUDE.md`.

## Module map

- `state-recovery.ts`: `startStateRecoveryNotices` / `stopStateRecoveryNotices` (main window only, wired in
  `routes/(main)/window-services.ts`), `announceStateRecovery` (one persistent warning per recovery).
- `StateRecoveryToastContent.svelte`: the words and actions per `StateFile`, in the security notices' look.
- Copy: `#lib/intl/messages/en/stateRecovery.json`. IPC: `#lib/tauri-commands/state-recovery.ts`.

## Must-knows

- **Drain, don't listen for content.** Start takes the queue, and every `state-recovery-queued` nudge takes it again;
  the nudge carries nothing. A recovery taken is gone, which is what makes it once.
- **Main window only**, ❌ never subscribe from another window: the settings window would drain the queue into a toast
  nobody sees.
- **One toast per recovery**, each its own id: two files breaking at once are two things to know.
- **The settings case points at Settings › Updates & privacy**, because usage stats and crash reports came back off; the
  license case at Settings › License. Every other file only offers Dismiss and Show in Finder.
- **The headline's key is a literal per `StateFile`** (an exhaustive `switch`), ❌ never built at runtime:
  `desktop-message-keys-unused` needs a real call site per key.

Decisions behind the shape: `DETAILS.md`.
