# Data-safety hunt 2026-10: archive verification

Verifier for `archive-hunt.md`. All seven excerpts checked against the current tree (`22918588b`); each is verbatim and
at the cited lines (give or take a line).

### archive-1

- **Verdict:** confirmed (every Cmdr-side step traced; the server-side reason the plain rename fails after the delete is
  the only untraced link, and the crash/force-quit window needs no server fault at all)
- **Severity:** high (agree)
- **Evidence:**
  - `crates/cmdr-operations/src/archive_edit/remote.rs:300-308`: the delete succeeds, then
    `rename(remote_temp, archive_path, false)` maps any failure through `vol_op(remote_temp)` to a bare `IoError`. No
    rescue, no `NewDataKeptAt`, and nothing deletes or renames the temp afterwards (`place_local_file` at `:146-167`
    just propagates with `?`).
  - The temp is `temp_sibling_path(remote_path)` (`remote.rs:155`), so its name carries `STAGING_TEMP_MARKER` =
    `.cmdr-tmp-` (`crates/cmdr-fs/src/staging.rs:93`).
  - `crates/cmdr-operations/src/transfer/volume/cleanup.rs:367-406`, `reap_stale_transfer_temps`: matches
    `!is_directory && name.contains(".cmdr-tmp-") && age >= 1 h` (`STALE_TEMP_MIN_AGE`, `:40`), with no prefix or ledger
    check, and deletes it. Called at the start of every volume copy (`transfer/volume/copy.rs:674`) and cross-volume
    move (`move_cross.rs:253`) into that directory. Its own doc (`cleanup.rs:352-356`) states the invariant this path
    breaks: "Nothing may leave committed data under a temp name."
  - Contrast `transfer/volume/finalize.rs:84-140`: the transfer swap checks `what_a_refused_delete_left` and, when the
    rename fails after a delete, calls `rescue_out_of_temp_space` and returns `new_data_at`. The archive swap has
    neither.
  - The archive's own `reap_remote_temps` (`remote.rs:330`, 24 h) only runs at the start of the next edit of the SAME
    archive, which the user can no longer open (the original name is gone), so it doesn't help; it would also delete the
    temp after 24 h if they recreated an archive by that name and edited it.
  - The module docs (`remote.rs:24-28`) claim the crash window is "never lost, only briefly misnamed"; the 1 h transfer
    reaper makes that false.
  - Second branch (forced rename applied on the server but answered with an error, then the fallback delete removes the
    NEW bytes): plausible only. Mechanism is right in code (`:291` discards the error and `:300` deletes
    unconditionally), but I didn't trace whether any backend can report an applied rename as failed.
  - No test in `remote_tests.rs` fails the rename after the delete.

### archive-2

- **Verdict:** confirmed
- **Severity:** high (agree)
- **Evidence:**
  - `crates/cmdr-operations/src/archive_edit/copy_into.rs:431-435`: a source dir only checks `!index.exists(&inner)`; no
    type check, no conflict, no `skipped_count` bump. Files under it go to `plan_file_add` (`:494+`), where
    `index.exists("notes/a.txt")` is false, so they're plain adds.
  - `DETAILS.md:359-360` documents this as intended ("just skips its `mkdir` and adds its children under the name"), so
    it's a design hole, not drift.
  - The mutator has no ancestor validation (`crates/cmdr-archive/src/mutation/mutator.rs`: no ancestor/collision check;
    `MutationError` has no such variant, `:147-173`). Retained entries are written first (`:289-297`), so the file
    `notes` precedes `notes/a.txt` in the new central directory.
  - `crates/cmdr-archive/src/read/index.rs:609-615` + `ensure_ancestors` (`:663-677`) + `upsert_dir`
    (`Some(_) => false`): a later entry under a FILE ancestor is dropped, and `:568` prunes its read handle. So the
    children are unlistable and unextractable through Cmdr.
  - The move: `should_delete = is_move && plan.skipped_count == 0` (`copy_into.rs:727`), then
    `materialized.delete_originals()` (`:741`). Source files are gone.
  - Only the opposite direction is pinned (`copy_into_tests.rs:716`). Bonus interaction with archive-4: a later delete
    or move-out of the file `notes` also deletes the hidden `notes/*` entries via the raw-prefix match.

### archive-3

- **Verdict:** confirmed
- **Severity:** high (agree; the corruption is reversible in principle since CP437 is a bijection, but no user will do
  that, and it hits every non-ASCII name in the archive)
- **Evidence:**
  - `mutator.rs:229`: `original_name = file.name()`; `:291-293`: `raw_copy_file_rename(file, entry.new_name.clone())`
    for every retained entry, touched or not.
  - `zip-9.0.1/src/read/zipfile.rs:83-85`: `ZipFile::name()` → `ZipFileData::name` (`types.rs:121-131`): UTF-8 if the
    raw bytes parse, else `from_cp437()`. No Unicode Path extra-field lookup there.
  - `zip-9.0.1/src/write.rs:1114-1137`: `raw_copy_file_rename` writes `name.to_string().as_bytes()` as the new raw name;
    even `raw_copy_file` (`:1195`) goes through `name()` too, so the crate's public raw-copy API can't preserve raw
    bytes. `types.rs:352-354` sets the UTF-8 (`LanguageEncoding`) flag for valid non-ASCII UTF-8, so the mojibake is now
    declared UTF-8 to every reader.
  - `rc-zip-5.4.1/src/fsm/archive.rs:305-353`: rc-zip only distinguishes UTF-8, Shift-JIS, and CP437. So for Shift-JIS
    archives Cmdr's own listing regresses (correct before, mojibake after). For GBK/CP866, Cmdr already showed CP437
    mojibake before the edit, so Cmdr's view doesn't change, but every external tool that decoded with the right code
    page breaks. The `0x5C` second-byte claim holds: CP437 maps `0x5C` to `\`, and the sanitizer maps `\` to `/`.
  - No encoding check in `crates/cmdr-archive/src/mutation/` or `archive_edit/` (grep for
    `utf8|name_raw|cp437|encoding`: no hits outside tests).

### archive-4

- **Verdict:** confirmed
- **Severity:** medium (agree; only malformed or hostile writers produce the hidden entries)
- **Evidence:**
  - `mutator.rs:658-688`: `plan_new_name` / `path_matches_or_under` match the raw `zip`-crate name, never sanitized;
    `"dir/../x"` and `"dir/f/g"` both match a `dir` delete.
  - `index.rs:494-497`: quarantined names never become nodes; `:609-615` and `:626` drop file-under-file and
    dir-shadowed files; so the move-out extract never sees them.
  - `move_out.rs:217-222`: the delete batch is the selected inner paths of cleanly landed sources, applied as prefixes.

### archive-5

- **Verdict:** plausible (index misalignment confirmed in both crates; the end-to-end move-out run is not traced)
- **Severity:** medium (agree)
- **Evidence:**
  - `zip-9.0.1/src/read/zip_archive.rs:229-241` pushes every CD record; `:75-78` folds them into an `IndexMap` keyed by
    raw name, so a duplicate replaces the earlier value at the earlier position and `len()` (`:360`) is n−1. No
    duplicate check in `src/read/`.
  - `crates/cmdr-archive/src/read/zip.rs:95-118` assigns the rc-zip enumeration index as `ordinal`; `:253-262` calls
    `by_index_decrypt(ordinal, ...)`. For any ordinal past the second duplicate, that opens the next entry; the CRC
    checked is the opened entry's own, so it streams cleanly.
  - Move-out deletion needs every encrypted entry in the archive to be in the move (else `EncryptedEntryRetained`
    refuses the rewrite and only the wrong-bytes extract remains). The hunter states this correctly. Not traced: whether
    the duplicate's later copy in rc-zip (`index.rs:550`, "later duplicate wins") leaves a selectable node whose ordinal
    lands past the shift, though by construction it does.

### archive-6

- **Verdict:** confirmed
- **Severity:** low (agree)
- **Evidence:** `mutator.rs:285`, `ZipWriter::new(temp_file)`; no `comment`/`set_raw_comment` anywhere in `mutator.rs`
  (grep: no hits). `src.len()` is the `IndexMap` length (`zip_archive.rs:360`), so the earlier duplicate is never
  iterated. A prefix (SFX stub) isn't copied, since the new file starts at the first local header.

### archive-7

- **Verdict:** plausible (the frontend navigation into the link isn't run, but each backend step supports it)
- **Severity:** low (agree; nothing is lost, the edit lands at the path the user edited)
- **Evidence:**
  - `crates/cmdr-fs/src/entry.rs:253`: `is_archive` is by name for any non-dir, so a symlink named `report.zip` lists as
    an archive.
  - `crates/cmdr-archive/src/boundary.rs:79-80` and `:127`: confirm with `std::fs::metadata` (follows links), so the
    link path confirms as a zip boundary.
  - `mutator.rs:277` mints the temp beside `archive_path` (the link), and `:359`
    `std::fs::rename(&temp_path, archive_path)` replaces the link itself. No `symlink_metadata` or `canonicalize` in
    `mutator.rs`, `engine.rs`, or `routing.rs`.
