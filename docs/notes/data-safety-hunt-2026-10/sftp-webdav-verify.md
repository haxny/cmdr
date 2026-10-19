# Data-safety hunt 2026-10: SFTP and WebDAV, verifier report (`sftp-webdav`)

Verifier pass over `sftp-webdav-hunt.md`. Read-only; nothing built or run. Every excerpt was checked against the current
tree and is verbatim at the cited line.

### sftp-webdav-1

- **Verdict:** duplicate of delete-1 (same root cause: the volume delete walker never asks `entry_kind` / `is_symlink`).
  The SFTP-specific claim is **confirmed**, and it corrects `delete-verify.md`: SFTP is SAFE for a link met INSIDE a
  selected folder, and AFFECTED when the link itself is the selected top-level item.
- **Severity:** critical (agree, same as delete-1)
- **Evidence, the top-level path for an SFTP volume:**
  - SFTP declines the listing oracle everywhere: `listing_watch_coverage` is `WatchCoverage::None`
    (`crates/cmdr-sftp/src/volume/volume_impl.rs:523`), and `try_get_authoritative_listing` returns `None` for that
    (pinned by `crates/cmdr-listing/src/caching_oracle_test.rs:69`). So there is no "warm preview" on SFTP at all; the
    preview's link-aware oracle branch (`entry.is_directory && !entry.is_symlink`, `scan_preview/volume.rs:371`) is
    unreachable for it.
  - With a preview: `scan_for_copy_batch_with_boundary` → `scan_walk::scan_trees` (`volume_impl.rs:365-372`) →
    `scan_tree_keeping`, whose first act is `source.scan_stat(path)` (`crates/cmdr-fs/src/volume/scan_walk.rs:92`).
    SFTP's `scan_stat` is `get_metadata_impl` (`crates/cmdr-sftp/src/volume/scan.rs:20`), which calls `fs().metadata`
    (`query.rs:86-93`), a following `stat`; the module says so itself at `query.rs:107-109` ("`get_metadata_impl` asks
    `stat`, which follows a link"). The link to a folder reads `is_directory: true`, so `top_level_is_directory: true`.
    The delete's cached-scan arm `Some(_)` (`delete/walker.rs:649-667`) calls `scan_volume_recursive(..., Some(true))`.
  - Without a preview: `parent_hint` (`walker.rs:741-757`) asks `authoritative_listing`, which is `None` on SFTP, so the
    walker gets `None` and probes `volume.is_directory(path)` (`walker.rs:350-356`). SFTP doesn't override
    `is_directory`, so the trait default (`crates/cmdr-fs/src/volume/mod.rs:319-324`) answers from `get_metadata`, the
    same following stat: `true`.
  - Either way the walker lists through the link (`walker.rs:429-432`, `list_directory_with_cancel`), queues
    `link/<child>` paths, and the delete phase removes them (`walker.rs:832-861` files, `:966` dirs). The server
    resolves `link/` mid-path, so the target's contents go. Last, `delete_impl("link")`
    (`crates/cmdr-sftp/src/volume/mutation.rs:149`) sends `SSH_FXP_REMOVE` first, which unlinks the link itself and
    succeeds. The op ends green; the target folder is left empty.
  - Why `delete-verify.md` said "Safe": it checked only the nested case. A link inside a walked folder comes from
    readdir, which `entry_of_type` maps as `is_directory: false, is_symlink: true`
    (`crates/cmdr-sftp/src/volume/mapping.rs:39-41`), so the walker queues it as a leaf and `SSH_FXP_REMOVE` unlinks
    only the link. That half is right. Its top-level paragraph analysed only `LocalPosixVolume`.
  - `entry_kind_impl` (`query.rs:110`, an `lstat`) exists and answers correctly; nothing on the delete path calls it.
- **Fix note for delete-1:** the fix has to cover the TOP of the walk too (the `None`-hint probe and
  `ScanSource::scan_stat` / `top_level_is_directory`), not only the per-child recursion, or SFTP stays exposed.

### sftp-webdav-2

- **Verdict:** confirmed
- **Severity:** high (agree)
- **Evidence:**
  - Enter really is the permanent overwrite on WebDAV. `RenameConflictDialog.svelte:30`:
    `enterResolution = supportsTrash ? 'overwrite-trash' : 'overwrite-delete'`, fired by the Enter handler at `:40-45`.
    `supportsTrash` comes from `paneOffersTrash` (`rename-flow.svelte.ts:422`, `pane/trash-availability.ts:25-27`),
    which reads the volume row; every server volume row has `supports_trash: false`
    (`apps/desktop/src-tauri/src/server_volumes.rs:192-210`, `:293`). Pinned by `rename-dialogs.test.ts:165-167` (Enter
    → `overwrite-delete` with no trash).
  - No type check anywhere on the way: `check_sibling_conflict_via_volume` (`rename/validity.rs:309-349`) reports any
    existing entry as a conflict; `rename-operations.ts:122-124` returns `conflict` without looking at `is_directory`;
    `handleConflictResolve` `overwrite-delete` → `performRename(…, true, …)` (`rename-flow.svelte.ts:697-700`);
    `rename_target` accepts any `Taken` name when forced (`crates/cmdr-operations/src/rename.rs:395-409`); `rename_work`
    defaults to `OneCall` (`cmdr-fs/src/volume/mod.rs:654-660`), so it reaches `volume.rename(&from, &to, true)`
    (`rename.rs:252`) → WebDAV `MOVE` with `Overwrite: T` (`crates/cmdr-webdav/src/volume/mutation.rs:149-160`).
  - The dialog's wording: title "A file with this name already exists", button "Overwrite and delete old file"
    (`intl/messages/en/fileExplorer.json:2077`, `:2121`), and a collection's size shows as 0 bytes. Nothing tells the
    user it's a folder or how much is in it.
  - Server side per RFC 4918 § 9.9.3 (not run here): `Overwrite: T` onto an existing collection deletes it with
    `Depth: infinity` before the move; the crate's own `DETAILS.md:458` records that `MOVE` overwrites and `DELETE` on a
    collection is recursive.
  - Why high, not critical: it takes an explicit confirm in a dialog naming the existing entry. But the consent is to
    overwrite "a file", it's the default key, and the loss can be a whole tree with no trash.
  - Worth checking in the fix: any other backend whose forced rename replaces a non-empty directory (SFTP doesn't:
    `remove_dir` refuses; local doesn't: the kernel refuses).

### sftp-webdav-3

- **Verdict:** plausible (the window is real in code; the race wasn't reproduced)
- **Severity:** medium (agree)
- **Evidence:** `crates/cmdr-webdav/src/volume/mutation.rs:121-145`: `PROPFIND Depth: 1` count check, then a separate
  `DELETE` with no `If` precondition. No lock, no ETag use on this path (`rg -i etag` in the crate's docs hits only the
  PROPFIND property list, `DETAILS.md:128`). The DELETE is recursive server-side, so a child that lands between the two
  requests goes with it and the op reports success. Needs a concurrent writer (sync client, auto-upload, second user)
  inside a one-round-trip window, so medium fits.

### sftp-webdav-4

- **Verdict:** confirmed
- **Severity:** medium (agree)
- **Evidence:** `lookup` (`crates/cmdr-sftp/src/known_hosts.rs:123-143`) only walks `matching(&addressed)` entries, so a
  `@revoked` entry is considered only when its host patterns match. `matches_pattern` (`:185-197`) returns `false` for
  any pattern with `*` or `?`, so `@revoked * ssh-ed25519 …` never matches. `decide` (`trust.rs:68-90`) then falls to
  Cmdr's own store (`Trusted` if approved before) or `Unknown` (one-click approval). Revocations for an exact or hashed
  host still work. Tests cover only exact-host revocations (`known_hosts_test.rs:32`, `:107-111`). The comment's "exact
  match is always safe" holds for positive entries only. Needs MITM with a revoked key, hence medium.

### sftp-webdav-5

- **Verdict:** confirmed; same pattern as `smb-3` (`smb-hunt.md`), and one fix shape should cover all three backends.
- **Severity:** low (agree). `smb-3` rates medium because SMB also SAVES the secret with Remember off; here the store
  helper only refreshes an already-remembered entry ("refreshes, never seeds"), so the loss is limited to overwriting a
  working secret with a typo.
- **Evidence:** WebDAV `do_reconnect_with_credentials` calls `refresh_remembered_secret` before `rebuild`
  (`crates/cmdr-webdav/src/volume/reconnect.rs:138-151`); SFTP does the same on the password, keyboard-interactive, and
  passphrase rungs (`crates/cmdr-sftp/src/volume/reconnect.rs:165-174`), before `transport::dial` inside `rebuild`
  (`:184-216`). Telling detail: WebDAV's failure log in `refresh_remembered_secret` (`reconnect.rs:286-291`) says "this
  connection still came up", the wording of a call meant to run AFTER a successful dial.

### sftp-webdav-6

- **Verdict:** confirmed
- **Severity:** low (agree)
- **Evidence:** `pump` (`crates/cmdr-sftp/src/volume/writes.rs:292-339`) returns whatever was written at source EOF and
  never compares it with `length`; `upload` (`:254-275`) closes and reports success. The engine declares
  `StreamLength::Known(size)` from the stream (`transfer/volume/stream_pipe.rs:229-233`) and takes `written.bytes` as is
  (`:439`); `source_reads::watch` only labels failed reads. The local read stream ends silently at a short EOF
  (`crates/cmdr-local/src/local_posix/streams.rs:42-70`), so a file that shrank mid-copy lands short (and one that grew
  lands long, the same missing check). The source still exists, hence low.

### sftp-webdav-7

- **Verdict:** plausible (code confirmed; a server that sends a 200 without `Content-Length` wasn't observed)
- **Severity:** low (agree)
- **Evidence:** `open_read_stream_impl` uses `response.content_length().unwrap_or(0)` for a 200
  (`crates/cmdr-webdav/src/volume/streams.rs:126-131`), and `total_size` reports it as `Known(total)` (`:80-81`). WebDAV
  doesn't override `open_read_stream_with_hint`, so the trait default discards the PROPFIND size hint
  (`cmdr-fs/src/volume/mod.rs:1500-1507`). `stream_pipe.rs:229-233` passes `Known(0)` on to the destination. The MTP
  consequence depends on `mtp-rs` and wasn't traced.

### sftp-webdav-8

- **Verdict:** confirmed
- **Severity:** low (agree)
- **Evidence:** every request carries Basic auth (`crates/cmdr-webdav/src/transport.rs:216-221`), `http://` is accepted,
  and no warning string exists in `lib/servers/` or the English catalogs (`rg -i "insecure|unencrypted|cleartext"` finds
  nothing relevant). Redirects are off (`transport.rs:141`), so an `https` URL can't be silently downgraded; the
  exposure is only for servers the user saved as `http://`.
