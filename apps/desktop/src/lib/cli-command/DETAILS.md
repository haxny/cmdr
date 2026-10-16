# The `cmdr` command's install toggle: decisions

Up: `CLAUDE.md`. The backend half, the transport, and the install flow: `src-tauri/src/cli/DETAILS.md`.

## One item, three faces

The menu item's label says what a click would do, so the click needs no confirmation:

- **`notInstalled`**: install. With `blockedBy: 'notInApplications'`, explain instead (move Cmdr to Applications first).
- **`installed`**: uninstall. Never blocked by where the copy sits: a moved Cmdr must still be able to take its link
  back.
- **`installedByHomebrew`** and **`takenByOther`**: explain, change nothing. Homebrew's link is `brew`'s; someone else's
  `cmdr` is theirs.
- **`blockedBy: 'notProductionBuild'`** (a dev, worktree, or E2E build): explain in every state. Such a build neither
  installs its own `target/` copy nor removes the installed app's link.

**Decision: a toggle, not two commands.** One menu slot whose label follows the filesystem reads cleaner than two items
where one is always greyed out, and it matches how VS Code words its own.

**Decision: the click re-reads the state before acting.** The label can be a focus-gain behind (the user changed things
from a terminal without leaving the window), so the frontend asks again, and the backend re-checks again before it
writes.

## Feedback

Every outcome gets a toast (`main.cliCommand.*`), at `success` for a change, `info` for nothing-to-do and explanations,
`warn` when the OS said no. One toast id, so a second click replaces the first toast.

Copy is a draft for David's review: the success toast teaches the first command to try (“cmdr .”).
