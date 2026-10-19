# Data-safety hunt 2026-10: `viewer` slice (verifier report)

All four excerpts are verbatim and current at `22918588b`.

### viewer-1

- **Verdict:** confirmed (Cmdr's side traced end to end; the app-side save is ordinary editor behavior, and is easier
  than the hunter says)
- **Severity:** high (hunter: medium). Per the scale, it's loss the user isn't told about and can't recover, needing a
  plausible condition (saving an edit into what they don't know is a temp copy). Not critical only because the editor
  shows a read-only hint first.
- **Evidence:**
  - `open_with_extract.rs:243` + `:263–272`: `make_read_only` touches only the FILE's mode bits, best-effort (failure
    logged at `debug`). The parent `.cmdr-open-with-<uuid>/` dir stays writable, so any editor that saves by
    write-temp-then-rename (vim `:w!`, Sublime's atomic save, BBEdit's safe save) replaces the file with no permission
    problem at all; the `0o444` bit only blocks in-place writers, and those offer an override.
  - `menu/open_with.rs:192–225` (`launch_with`): the success path calls `open_paths_with(&copies, …)` and nothing else;
    no event, no toast. The only `fileExplorer.openWith.*` strings are `copyRefused.*`
    (`src/lib/intl/messages/en/fileExplorer.json:1310–1345`). The user is never told the app got a temp copy.
  - `lib.rs:382` → `init_open_with_extract_dir` (`open_with_extract.rs:78`) → `reap_open_with_temps` →
    `reap_temps_with_prefix` (`materialize.rs:162–177`): unconditional `remove_dir_all` of every `.cmdr-open-with-*`
    entry at every start. No mtime/size check against the pull, no "more than one file in the subdir" check, no Trash,
    failures logged at `debug` only, success not logged at all.
  - `file_viewer/DETAILS.md:295–298` documents "write-back (edits an app saves stay in the copy)" as intentional;
    nothing documents that those edits are then deleted.
  - No guard found: no test pins a "modified copy survives" rule; the reaper's only filter is the name prefix.
- **Extra trigger the hunter didn't list:** Cmdr's own silent auto-update restart (or any relaunch) while the app still
  has the copy open. The reaper removes the dir under the open document, so a later in-app save either fails (dir gone,
  for an atomic-save editor) or writes to an unlinked inode; either way the edits don't survive.

### viewer-2

- **Verdict:** confirmed
- **Severity:** low (agree)
- **Evidence:**
  - `open_with_extract.rs:219–247`: each launch pulls a fresh uuid subdir; the
    `.map(...).collect::<Result<Vec<_>, _>>()` short-circuits on the first `RefusedCopy`, so rows 1..N-1's copies stay
    on disk with no app ever opening them (`launch_with` returns at `menu/open_with.rs:213` without cleanup).
  - Only reaper is the startup one (`open_with_extract.rs:78`); the `OPEN_WITH_CAP_BYTES` comment (`:66–69`) bounds one
    copy, not the sum.
  - Space comes back at restart, so no user data lost; ENOSPC pressure on other apps is real but indirect.

### viewer-3

- **Verdict:** confirmed
- **Severity:** low (agree)
- **Evidence:**
  - `encoding.rs:427–448`: UTF-8 path is `from_utf8_lossy`; other encodings drop `_had_errors`.
  - `encoding.rs:181–202` (`detect_from_head`): detection looks at the head only, so a large log whose stray
    Latin-1/binary bytes sit past the head stays `Utf8`, and every invalid sequence becomes U+FFFD in the save.
  - `session.rs:960–987` writes the decoded UTF-8 and returns `Ok(())`; nothing carries a lossy flag. The source file is
    untouched, so nothing is irrecoverable.

### viewer-4

- **Verdict:** confirmed
- **Severity:** low (agree)
- **Evidence:**
  - `range_read.rs:139–152`: `finish` → `flush` → the sink's `write_all`; after it returns, `stream_range` returns `Ok`
    (`range_read.rs:352–364`) with no cancel check.
  - `session.rs:972–986`: `with_registered_read` (`session.rs:845–869`) returns the body's result without consulting the
    flag, then `std::fs::rename(tmp_path, dest_path)` runs unconditionally on `Ok`.
  - Mid-stream checks are only every 256 rows / 64 KiB (`range_read.rs:256–257`, `:303`), so the window is wider than
    "the last 1 MiB write": a stall in any flush with less than 64 KiB of output left to the next check, followed by
    completion, also renames.
  - `commands/file_viewer.rs:469–482` (`SaveWatch::give_up`) flips the flag and returns `TimedOut`; its comment's
    promise ("removes its temp file, and answers `Cancelled`") doesn't hold here. The destination was confirmed in the
    native save panel, so the harm is the false failure report.
