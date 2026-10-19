# zip 9.0.1, patched

The crates.io release of `zip` 9.0.1 (upstream `zip-rs/zip2` at tag `v9.0.1`), swapped in through the root
`Cargo.toml`'s `[patch.crates-io]`. Everything here is the published package byte for byte, apart from the change below
and the files left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.
- `.cargo-ok`, `.devcontainer/`, `.pre-commit-config.yaml`, `.whitesource`: registry and upstream dev tooling, nothing
  the build reads.

The `zip` 4.6.1 that `tauri-plugin-updater` pulls in stays on crates.io: a `[patch]` only replaces the line it vendors,
and that copy only reads update bundles.

## The change

- `src/write.rs`: `raw_copy_file` and `raw_copy_file_touch` write the entry's `name_raw()` under its own language
  encoding flag (general purpose bit 11). `raw_copy_file` went through `name()` before, which reads a name lacking the
  flag as CP437, and the writer then stored that reading as UTF-8 and flagged it. `raw_copy_file_rename` still takes the
  new name as UTF-8 and derives the flag from it. Its options-building half moved into `raw_copy_options`, shared by both.
  `deep_copy_file` and `shallow_copy_file` set the flag for their destination name.
- `src/format/functions.rs` and `src/types.rs`: the writer takes bit 11 from the entry's stored flags (`get_flags`)
  rather than re-deriving it from the name bytes at write time; every path that gives an entry a name sets it
  (`needs_language_encoding`, `ZipFileData::set_language_encoding_for`). `ZipRawValues` carries a raw copy's own flag
  into `start_entry`. `src/format/flags.rs`: `ZipFileFlags::set`.
- Unit tests at the end of `src/write.rs`'s `tests` module, the same ones as upstream.
- Known limit: a raw copy re-encodes a non-ASCII entry COMMENT as UTF-8 (the reader keeps no raw comment bytes), so a
  CP437 comment under a clear flag reads back garbled. Entry comments are rare; names were the data loss.

## Why

Zips from Japanese, Chinese, or Russian Windows tools store Shift-JIS, GBK, or CP866 names without the UTF-8 flag. Any
Cmdr edit of such a zip rewrote every kept entry through `raw_copy_file_rename(file, name())`, turning every non-ASCII
name in the archive into flagged mojibake, some split into fake folders by a `0x5C` byte (#514, verified on 9.0.1 and
upstream `master` at `f8a0fdb6`, 2026-10-20). Re-deriving the flag from the bytes isn't enough on its own: a Shift-JIS
or GBK name can be valid UTF-8 by accident, and old Info-ZIP writes UTF-8 without the flag. Cmdr's tests for the
property: `crates/cmdr-archive/src/mutation/mutator_raw_names_test.rs`.

The same change is drafted for upstream on a `preserve-raw-names-in-raw-copy` branch off `master`.

## Dropping it

When upstream ships a raw copy that keeps the stored name, delete this folder, its `[patch.crates-io]` line, and its
`exclude` entry, and keep the tests. `desktop-rust-vendor-patch-applied` fails as soon as a bump past 9.0.1 makes cargo
resolve crates.io again.
