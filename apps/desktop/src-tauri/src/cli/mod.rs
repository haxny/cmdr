//! The `cmdr` terminal command: `cmdr <path>` shows a path in Cmdr, like VS Code's `code`.
//!
//! The command itself is a shell script shipped in the bundle (`resources/bin/cmdr`). It
//! resolves the paths and sends a `cmdr-cli://open?…` URL to this exact bundle with
//! `open -a`, which reaches us as `RunEvent::Opened`, the same door a Finder reveal uses
//! (`crate::reveal`), so cold launch, buffering, and the pane move are shared.
//!
//! Two parts live here: `open_url` parses and validates what the script sends, and `install`
//! owns the app menu's "Install 'cmdr' command" link in `/usr/local/bin`. macOS-only, like the
//! delivery path it feeds. `DETAILS.md` for the decisions.

pub(crate) mod open_url;

#[cfg(test)]
mod script_tests;
