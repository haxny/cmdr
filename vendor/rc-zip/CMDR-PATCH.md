# rc-zip 5.4.1, patched

The crates.io release of `rc-zip` 5.4.1 (upstream `bearcove/rc-zip` at `373fa9bb`, tag `rc-zip-v5.4.1`, path `rc-zip`),
swapped in through the root `Cargo.toml`'s `[patch.crates-io]` and repeated in `fuzz/Cargo.toml`'s. Everything here is
the published package byte for byte, apart from the changes below and the one file left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.

## The changes

- `Cargo.toml`: the optional `zstd` dependency moves from `0.13.0` to `0.14.0`. The `zstd` 0.13 line (with `zstd-safe`
  7) carries RUSTSEC-2026-0391 and 0395–0406, and 0.14 clears them. No API change reached rc-zip's code (`Decoder`,
  `write`, `flush`, `into_inner` all stand). Upstream PR `bearcove/rc-zip` #173 (open, checked 2026-10-26) makes the
  same one-line bump.
- `src/fsm/entry/zstd_dec.rs`: a zstd entry whose decoder takes only part of a call's input now hands out what it
  decoded and lets the entry fsm feed the rest. The decoder stops taking input once its 32 KiB output buffer fills, and
  upstream's leftover check compared the bytes taken against what was LEFT rather than what was fed, so on the last
  read of an entry (`HasMoreInput::No`) any leftover beyond one byte failed as "expected ZSTD trailer or no ZSTD
  trailer, but not a 12-byte trailer". So a zstd (method 93) entry big enough to fill that buffer failed to read (a
  200 KB one does), on `zstd` 0.13 the same as on 0.14 (verified on 5.4.1 with Cmdr's test below, 2026-10-26; upstream
  `master` at `15fb2e34` has the same code). The one-byte trailer upstream tolerates (its `found-me-zstd.zip` corpus
  case) still reads: upstream's own test suite passes with the change (`cargo test --workspace --all-features` at the
  tag, 2026-10-26). Cmdr's test: `crates/cmdr-archive/src/read/archive_test.rs`, `reads_zstd_compressed_entry`.

## Dropping it

When an rc-zip release depends on `zstd` 0.14 or later and reads a large zstd entry (run `reads_zstd_compressed_entry`
against it), delete this folder, its two `[patch.crates-io]` lines (root and `fuzz/`), and its `exclude` entry, and keep
the test. `desktop-rust-vendor-patch-applied` fails as soon as a bump past 5.4.1 makes cargo resolve crates.io again.
