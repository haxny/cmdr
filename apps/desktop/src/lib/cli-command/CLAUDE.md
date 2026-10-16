# The `cmdr` command's install toggle (frontend)

What a click on the app menu's "Install “cmdr” command" (or its palette row, `app.toggleCliCommand`) does, and the toast
after it. Everything that touches `/usr/local/bin` lives in Rust: `src-tauri/src/cli/CLAUDE.md`.

## Module map

- **`cli-command-plan.ts`**: pure. `planCliCommandClick` (install, uninstall, or explain why not) and
  `cliCommandFeedback` (the toast for an outcome).
- **`toggle-cli-command.ts`**: the flow (read status → act → toast), plus `refreshCliCommandName` for the palette label,
  called at startup (`window-services.ts`) and on every main-window focus-gain (`listener-setup.ts`).

## Must-knows

- **Every decision reads the typed `CliCommandStatus` / `CliCommandOutcome`, ❌ never a message.** A `refused` outcome
  is worded from the status it came back with, which can have moved since the click.
- **The palette label and the native menu label are two mechanisms over one fact**: the palette's comes from
  `updateCliCommandName` (a `nameKey` thunk in `#lib/commands/sources/app.ts`), the menu's from Rust
  (`cli::commands::refresh_menu_item`). Both re-read on focus-gain.
- **The admin prompt's sentence is worded here** (`main.cliCommand.adminPrompt*`) and passed to Rust, so macOS's
  password dialog speaks the user's language.
- **The handler is `detached`**: an install can sit on the password dialog for minutes.

Decisions and copy notes: `DETAILS.md`.
