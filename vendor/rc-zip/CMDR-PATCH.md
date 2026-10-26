# rc-zip 5.4.1, patched

The crates.io release of `rc-zip` 5.4.1 (upstream `bearcove/rc-zip` at `373fa9bb`, tag `rc-zip-v5.4.1`, path `rc-zip`),
swapped in through the root `Cargo.toml`'s `[patch.crates-io]` and repeated in `fuzz/Cargo.toml`'s. Everything here is
the published package byte for byte, apart from the change below and the one file left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.

## The change

- `Cargo.toml`: the optional `zstd` dependency moves from `0.13.0` to `0.14.0`. The `zstd` 0.13 line (with `zstd-safe`
  7) carries RUSTSEC-2026-0391 and 0395–0406, and 0.14 clears them. No API change reached rc-zip's code (`Decoder`,
  `write`, `flush`, `into_inner` all stand). Upstream PR `bearcove/rc-zip` #173 (open, checked 2026-10-26) makes the
  same one-line bump.

## Dropping it

When an rc-zip release depends on `zstd` 0.14 or later, delete this folder, its two `[patch.crates-io]` lines (root and
`fuzz/`), and its `exclude` entry. `desktop-rust-vendor-patch-applied` fails as soon as a bump past 5.4.1 makes cargo resolve crates.io again.
