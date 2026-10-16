# The `cmdr` terminal command

`cmdr <path>` in a terminal shows that file or folder in Cmdr, like VS Code's `code`. The command is a POSIX `sh`
script shipped in the bundle (`../../resources/bin/cmdr`, at `Cmdr.app/Contents/Resources/bin/cmdr`); this module is
the app's half. macOS-only, gated in `lib.rs`.

## Module map

- **`open_url.rs`**: `parse_cli_url`, the one parser for the `cmdr-cli://open?path=…&pane=…&tab=new` URL the script
  sends. Feeds `reveal::delivery`, which owns the door, the buffer, and the pane move.
- **`install.rs`**: the app menu's "Install “cmdr” command" state machine (`CliInstall`) over the `CommandHost` seam:
  a symlink at `/usr/local/bin/cmdr`. **`system_host.rs`**: the real host (fs, `Info.plist`, the `osascript` admin
  prompt). **`commands.rs`**: three IPC commands and the menu item's live label.
- **`script_tests.rs`**: runs the real script with `--print-url` and parses what it prints, so encoder and parser
  can't drift.

## Must-knows

- **The `cmdr-cli` scheme is deliberately NOT registered** (no `CFBundleURLTypes`). The script names our bundle with
  `open -a`, which delivers an unclaimed scheme; a bare link from a web page or another app gets "no application knows
  how to open URL". ❌ Don't register it. Even so, treat the URL as hostile: it may only ever navigate.
- **A terminal open must never emit `RevealDelivered`** (it would spend the once-ever Finder notice). The origin rides
  `RevealRequest.origin`.
- **The install state reads through to the filesystem, ❌ never a stored setting**: `rm`, `brew install`, or a moved
  Cmdr all change it from outside.
- **Never touch someone else's `cmdr`.** Install is a plain `ln -s` (fails rather than replaces); uninstall removes only
  a link into a Cmdr bundle, and its root step re-checks the target right before `rm`. Homebrew's link is `brew`'s.
- **Same build gate as the reveal handler** (`own_bundle_id`): a dev build changes nothing in either direction, and a
  copy outside Applications may not install (but may always uninstall).
- **The admin prompt is a spawned `osascript`**, ❌ never in-process `NSAppleScript` (main thread only: it would freeze
  the app while the password dialog is up). Its reply is a number we print, ❌ never parsed message text.
- **The script and `open_url.rs` share `SCHEME`, and the script path is pinned in three places**: `tauri.conf.json`
  `bundle.macOS.files`, `install::BUNDLE_SCRIPT`, and the cask's `binary` stanza.

Architecture, the transport decision and its evidence, and the install flow: `DETAILS.md`.
