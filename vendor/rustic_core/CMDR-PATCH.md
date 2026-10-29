# rustic_core 0.13.0, patched

The crates.io release of `rustic_core` 0.13.0 (upstream `rustic-rs/rustic_core` at `1030be45`, path `crates/core`),
swapped in through the root `Cargo.toml`'s `[patch.crates-io]`. Everything here is the published package byte for
byte, apart from the change below and the one file left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.

## The change

- `Cargo.toml` and `Cargo.toml.orig`: the `zstd` dependency moves from `0.13.3` to `0.14.3`. The `zstd` 0.13 line (with
  `zstd-safe` 7) carries RUSTSEC-2026-0391 and 0395–0406, and 0.14 clears them. No API change reached rustic_core's
  code: it builds unchanged, and Cmdr's restic tests, which read real zstd-compressed v2 repos, pass
  (`cargo nextest run -p cmdr-restic`, 2026-10-26). No upstream commit, PR, or issue moves it yet (checked 2026-10-26).

## Dropping it

When a rustic_core release depends on `zstd` 0.14 or later, delete this folder, its `[patch.crates-io]` line, and its
`exclude` entry. `desktop-rust-vendor-patch-applied` fails as soon as a bump past 0.13.0 makes cargo resolve crates.io
again.
