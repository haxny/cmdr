# tauri-plugin-store 2.5.0, patched

The crates.io release of `tauri-plugin-store` 2.5.0 (upstream `tauri-apps/plugins-workspace` at `a2364a5f`, path
`plugins/store`), swapped in through the root `Cargo.toml`'s `[patch.crates-io]`. Everything here is the published
package byte for byte, apart from the change below and two files left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.
- `LICENSE.spdx`: `osv-scanner` reads any `*.spdx` as an SBOM and stops on this one. The license texts and copyright
  stay in `LICENSE_MIT` and `LICENSE_APACHE-2.0`.

## The change

- `src/store.rs`: `StoreInner::save` calls a new `write_atomically` instead of `fs::write`. It writes a uniquely named
  sibling temp file (`.<name>.<pid>.<n>.tmp`), syncs it, renames it over the store file, and syncs the parent directory.
  A symlinked store file is resolved first. On failure it removes the temp file and returns the error.

## Why

Upstream's `fs::write` truncates the file and then writes it, with no sync, so a crash, a forced quit, or a full disk
mid-save leaves `settings.json`, `shortcuts.json`, or `license.json` truncated or empty (#461, verified against upstream
`v2` and 2.5.0 on 2026-10-16). Cmdr's own tests for the property: `apps/desktop/src-tauri/src/settings/plugin_store_tests.rs`.

## Dropping it

When upstream ships an atomic save, delete this folder and the `[patch.crates-io]` entry, and keep the tests.
`desktop-rust-vendor-patch-applied` fails as soon as a bump past 2.5.0 makes cargo resolve crates.io again.
