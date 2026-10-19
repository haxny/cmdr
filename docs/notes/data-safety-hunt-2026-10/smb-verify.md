# SMB verify (slug `smb`)

Verified against the worktree at `22918588b` and smb2 `0.31.1` (the `Cargo.lock` pin, read from the cargo registry copy,
so paths below are crate-relative: `src/client/tree.rs`), plus smb2 `HEAD` (`v0.32.1`, `~/projects-git/vdavid/smb2`) for
what a fix could use. All excerpts are verbatim and current; cited lines match (the `reconnect_volume_with_credentials`
fn is at `commands/network.rs:641`, one off).

### smb-1

- **Verdict:** plausible (Cmdr side confirmed end to end; the server-side junction resolution is documented, not
  tested). Not a duplicate of delete-1: the two need separate fixes, and fixing either alone doesn't close smb-1.
- **Severity:** high (agree). Silent deletion outside the selection, but needs a Windows server share holding a
  junction, which is rarer than the macOS/NAS setups most users have; Samba doesn't make junctions.
- **Evidence:**
  - `crates/cmdr-smb/src/volume/mapping.rs:27`: `FileEntry::new(entry.name.clone(), path, entry.is_directory, false)`;
    `is_symlink` is hardcoded `false`, and `rg -i 'reparse|symlink' crates/cmdr-smb/src` hits only
    `mapping_test.rs:35,54`, which assert `!fe.is_symlink`.
  - smb2 0.31.1 `DirectoryEntry` (`src/client/tree.rs:159`) exposes `attributes: FileAttributes`, so
    `FileAttributes::REPARSE_POINT` (`src/types/flags.rs:363`, `0x400`) is available to Cmdr today. The reparse TAG is
    not exposed: smb2 parses `ea_size` (which carries the tag when `REPARSE_POINT` is set, MS-FSCC 2.4.17) at the msg
    layer (`src/msg/directory_info.rs:84`) but drops it from `DirectoryEntry`. `FileInfo` (stat) also carries
    `attributes` (`tree.rs:281`).
  - smb2 sends no `FILE_OPEN_REPARSE_POINT` (`0x00200000` create option) anywhere, at 0.31.1 or `HEAD`:
    `delete_compound` uses `create_options: type_option` (`tree.rs:2516`), `delete_file` passes
    `FILE_NON_DIRECTORY_FILE` (`tree.rs:1006`), `delete_directory` `FILE_DIRECTORY_FILE` (`tree.rs:2482`), and the
    listing opens with `FILE_DIRECTORY_FILE`. There's no public open that takes custom create options, so "remove the
    link, keep the target" needs an smb2 change.
  - `crates/cmdr-operations/src/delete/walker.rs:440–442` recurses on `child.is_directory` alone (cached or fresh
    listing, both built by the same mapping). Non-local deletes route here (`local_start.rs:353`). `delete_impl`
    (`mutation.rs:101`) tries `delete_file`, then `delete_directory` on `IsADirectory`, with no stat.
  - Server semantics: Microsoft's answer on MS-SMB2 CREATE handling (learn.microsoft.com Q&A 2280771, 2025-06) says a
    server opening without `FILE_OPEN_REPARSE_POINT` opens the reparse point's TARGET and flags
    `SMB2_CREATE_FLAG_REPARSEPOINT` in the response, while a symlink returns `STATUS_STOPPED_ON_SYMLINK` because
    "Windows-based servers will never follow a symlink". A junction is a mount-point reparse point, not a symlink, so it
    falls in the first case: listing and deleting through it act on the target. (Doc-based; no live server run.)
  - Resulting shape: target's files deleted through `folder/junction/<file>`, then `delete_directory(folder/junction)`
    opens and deletes the now-empty TARGET dir; the junction itself survives, so the parent's final `rmdir` fails
    `DIRECTORY_NOT_EMPTY`, which is a visible error but after the loss.
- **Relationship to delete-1:** delete-1's fix (the walker honoring `is_symlink`) does nothing here while SMB reports
  `false`; smb-1's mapping fix alone does nothing while the walker ignores `is_symlink`. The hunter's "every walker
  already treats it as a leaf" holds for `scan_walker.rs` but not for the delete walker, so smb-1's fix depends on
  delete-1's. And even with both, a leaf junction deleted via today's `delete_impl` hits the target
  (`DIRECTORY_NOT_EMPTY` if non-empty: safe; if the target is empty, the target dir goes and the link stays), so the
  smb2 `FILE_OPEN_REPARSE_POINT` delete is a third, SMB-only piece.
- **Fix caveat the hunter's suggestion misses:** mapping ALL `REPARSE_POINT` to `is_symlink` over-matches. Windows
  Server dedup files, OneDrive/cloud-files placeholders (files AND folders), and other non-link tags carry the bit too;
  treating a cloud-tagged real folder as a link would make delete open it as a reparse point and fail as non-empty, and
  copy would skip its contents. Expose the tag from smb2 (`ea_size`) and treat only `IO_REPARSE_TAG_MOUNT_POINT` /
  `IO_REPARSE_TAG_SYMLINK` as links.

### smb-2

- **Verdict:** confirmed.
- **Severity:** medium (hunter: high). The source survives and the copy reports a failure (a taken name) for that file,
  so the loss is recoverable and the user is told something went wrong. It needs three things together: a source still
  being written, growth past `max_write` during the read, and a drop or quit within a few round trips. Medium per the
  brief's "recoverable loss".
- **Evidence:**
  - `crates/cmdr-local/src/local_posix/streams.rs:43–75`: `next_chunk` reads 1 MiB chunks until `read()` returns 0
    (EOF); `total_size()` (`:77`) is the open-time `stat`. Nothing caps at `total_size`.
  - No wrapper caps it either: `source_reads.rs:55` forwards chunks, `checkpoint_stream.rs:122–134` only counts
    `bytes_yielded` for progress, `quarantine_stream` adds metadata.
  - `stream_pipe.rs:248–253`: `write_is_single_shot(length)` decides staging; SMB says yes for
    `size <= quick_write_limit` (`streams.rs:426–433`), so `resolve_staging` gives `SingleShot`, no temp.
    `stream_pipe.rs:287` disarms the stall watchdog for it.
  - `streams.rs:524–620`: the compound branch drains the whole stream, re-checks
    `fits_one_compound_write(limit, buffer.len())` at `:553` against `limit = max_write` for a real name
    (`one_frame_write_limit`, `:111`), and on overflow falls through to `open_file_writer(&tree, conn, &smb_path, mode)`
    at `:620`: a multi-round-trip write at the real name. That contradicts both the
    `❌ keep the drained buffer on this path` comment at `:512` and the `cmdr-smb/CLAUDE.md` must-know. The
    `CreditStarvation` exception at `:570` is scratch-only and unrelated.
  - On a `write_chunk` error the code does `writer.abort()` + `delete_partial()` (`:625–631`), but `delete_partial`
    (`:491`) needs `clone_session()`, which fails once `handle_smb_result` has marked the volume disconnected, so
    nothing is deleted.
  - Engine side, as claimed: `create_went_unanswered` (`interrupted_landing.rs:123`) is true for a retryable `CreateNew`
    failure; `abandon_attempt` (`staged_write.rs:433`) leaves `SingleShot` targets alone; `settle_unanswered_create`
    (`interrupted_landing.rs:133`) returns `RunAgain` on a size mismatch and "leaves the name alone"; the next
    `CreateNew` meets our own partial. The `HardAborted` arm's comment (`stream_pipe.rs:335–340`, "dropping it leaves
    either the whole file or nothing") is also falsified by this path.
- **Fix note:** the fix belongs in `streams.rs` (refuse a real-name buffer that outgrew `size`, or cap the drain at
  `size`, with a typed error the engine retries staged). Capping at the local source alone wouldn't cover other sources
  whose stream outruns `total_size`.

### smb-3

- **Verdict:** confirmed.
- **Severity:** medium (agree).
- **Evidence:**
  - `crates/cmdr-smb/src/volume/reconnect.rs:272–301`: `save_credentials(&server, None, &stored)` runs first and
    unconditionally, then params are replaced, then `do_attempt_reconnect`. Nothing reads or keeps the previous entry.
  - The only caller is `commands/network.rs:641` ← `open-sign-in.ts:457` (`mendAttempt`), wrapped in `withRememberFlip`
    (`open-sign-in.ts:208`). With Remember turned off, `withRememberFlip` calls `forgetServerSecret` (`:246`) BEFORE the
    attempt, then the SMB backend re-seeds the entry on the next line, which is the exact case the `withRememberFlip`
    doc (`:222–230`) says the write-first ordering exists to prevent.
  - The siblings do the opposite: `cmdr-fs/src/volume/secret_store.rs:39` `refresh_remembered_secret` writes only where
    an entry already exists (`:49`), used by S3 (`cmdr-s3/src/volume/reconnect.rs:279`) and, per the hunter,
    SFTP/WebDAV.
  - The Rust doc comment calls the save deliberate ("mirroring how the login form saves"), but the login form saves
    after success and only when remembered (`smb_connect_directly.rs:289`, per the hunter), so it doesn't mirror it.

### smb-4

- **Verdict:** confirmed.
- **Severity:** low (agree).
- **Evidence:**
  - `network/known_shares.rs:182–186`: `serde_json::from_str(&contents).unwrap_or_default()`;
    `network/manual_servers.rs:458–465` same shape. No backup, no refusal to save.
  - `ConnectionMode` (`known_shares.rs:17`) and `AuthOptions` (`:25`) have no `#[serde(other)]` (`rg 'serde\(other\)'`
    finds none in either file), so one unknown variant fails the whole document.
  - What's lost beyond a re-entry: `pinned` (`:72`), `direct_connection_opt_outs` (`:107`), and `pending_moves`
    (`:117`), which `server_move` uses to re-key a moved share on its first mount (`:378–385`); losing it leaves
    favorites and tabs naming the old volume id. Keychain secrets stay. Trigger is a downgrade only, which auto-update
    makes rare.
