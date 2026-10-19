# SMB hunt (slug `smb`)

Scope: `crates/cmdr-smb/`, `apps/desktop/src-tauri/src/network/`, `server_move.rs`, `server_move_smb.rs`. smb2 read at
the pinned `v0.31.1` tag (`Cargo.lock`), not its `HEAD`.

Four findings survive, ranked: one high (plausible), one high (confirmed), one medium, one low.

### smb-1. Deleting a folder on a Windows share that holds a junction empties and removes the junction's TARGET, outside the selection

- **Severity:** high
- **Confidence:** plausible (Cmdr's side traced end to end; the Windows server resolving a junction server-side on an
  open without `FILE_OPEN_REPARSE_POINT` is documented SMB behavior, not tested here)
- **Where:** `crates/cmdr-smb/src/volume/mapping.rs:20`, `directory_entry_to_file_entry`;
  `crates/cmdr-operations/src/delete/walker.rs:440`, `scan_volume_recursive`;
  `crates/cmdr-smb/src/volume/mutation.rs:101`, `delete_impl`; smb2 `v0.31.1` `crates/smb2/src/client/tree.rs:2482`
  (`delete_directory`) and `:2512` (`delete_compound`, `create_options: type_option` only)
- **Excerpt:**

```rust
// crates/cmdr-smb/src/volume/mapping.rs:20
pub(super) fn directory_entry_to_file_entry(
    entry: &smb2::client::tree::DirectoryEntry,
    parent_path: &str,
) -> FileEntry {
    let path = if parent_path.ends_with('/') {
        format!("{}{}", parent_path, entry.name)
    } else {
        format!("{}/{}", parent_path, entry.name)
    };

    let mut fe = FileEntry::new(entry.name.clone(), path, entry.is_directory, false);
    fe.size = if entry.is_directory { None } else { Some(entry.size) };
```

```rust
// crates/cmdr-operations/src/delete/walker.rs:440
        for child in &children {
            let child_path = PathBuf::from(&child.path);
            if child.is_directory {
                Box::pin(scan_volume_recursive(
                    volume,
                    volume_id,
                    &child_path,
                    Some(true),
```

- **Mechanism:**
  1. smb2's `DirectoryEntry` carries `attributes` (with `FILE_ATTRIBUTE_REPARSE_POINT`, `0x400`, `types/flags.rs:363`),
     and `is_directory` is `attributes.is_directory()`. A junction (`IO_REPARSE_TAG_MOUNT_POINT`) lists as
     `DIRECTORY | REPARSE_POINT`.
  2. Cmdr drops the attributes: every SMB `FileEntry` is `is_symlink = false` (`mapping.rs:30`, `query.rs:201`), and
     `rg -i 'reparse|symlink' crates/cmdr-smb/src` returns nothing.
  3. The volume delete walker recurses into every `child.is_directory` (`walker.rs:442`). Its listing of
     `folder/junction` opens the path without `FILE_OPEN_REPARSE_POINT`; Windows resolves a junction on the SERVER (only
     true symlinks come back as `STATUS_STOPPED_ON_SYMLINK`), so the listing is the target folder's contents, and
     they're collected under `folder/junction/...`.
  4. Delete runs files first, then dirs deepest-first: each `folder/junction/<file>` is a `delete_file` that again
     resolves through the junction, deleting the target's real files. Then `delete_impl(folder/junction)`: `delete_file`
     (`FILE_NON_DIRECTORY_FILE`) opens the TARGET, answers `IsADirectory`, and `delete_directory` sets the delete
     disposition on the now-empty TARGET directory itself.
- **Trigger:** A Windows PC or Windows Server share where someone made a junction (`mklink /J`, common for relocating
  game libraries, Steam/Dropbox folders, build caches, or an old profile's `My Documents`-style links). The user selects
  a parent folder in Cmdr and presses Delete (or does a cross-volume move from that share, whose source sweep deletes
  the ledgered files through the same junction paths).
- **Guard ruled out:** The local walker uses `symlink_metadata`, but the volume walker only has `FileEntry`;
  `scan_walker.rs:380` checks `!entry.is_symlink`, always `false` for SMB. `delete_impl` doesn't stat first and smb2
  sends no `FILE_OPEN_REPARSE_POINT` on delete or list. The delete preview shows the counts, but the junction contents
  look like ordinary files under the selected folder. Windows Explorer avoids this exactly by checking the reparse
  attribute and removing only the link. No test or doc in `cmdr-smb` or `delete/` mentions reparse points.
- **Impact:** Permanent deletion of a folder tree OUTSIDE the selection (the junction's target, which can be anywhere on
  the server's local volumes the server process can open). Not recoverable from Cmdr (delete isn't rollbackable; SMB has
  no trash).
- **Suggested fix:** Map `FILE_ATTRIBUTE_REPARSE_POINT` to `FileEntry.is_symlink = true` (and in `get_metadata_impl`
  from the stat's attributes), so every walker already treats it as a leaf; and delete such a leaf with an smb2 open
  that carries `FILE_OPEN_REPARSE_POINT`, so the link goes and its target stays.

### smb-2. A small file that grows while it's read gets streamed straight to its real name on the share, so a dropped connection leaves a partial there wearing the user's filename

- **Severity:** high
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-smb/src/volume/streams.rs:528`, `write_from_stream_impl` (the compound branch's fall-through
  at `:610`); `crates/cmdr-local/src/local_posix/streams.rs:43`, `LocalPosixReadStream::next_chunk` (reads to EOF, not
  to `total_size`); `crates/cmdr-operations/src/transfer/volume/stream_pipe.rs:248`, `:287`, `:342`;
  `crates/cmdr-operations/src/transfer/staged_write.rs:433`, `abandon_attempt`
- **Excerpt:**

```rust
// crates/cmdr-smb/src/volume/streams.rs:551
                    // The drained bytes, not the promised count, decide: a
                    // source that yielded short still goes out as one frame.
                    if fits_one_compound_write(limit, buffer.len() as u64) {
                        ...
                    }
                    // The source yielded MORE than one frame can carry (it
                    // reported a smaller size than it had), or the window
                    // refused the frame for a staging temp. Feed the drained
                    // buffer through the streaming writer on the same cloned
                    // connection. No lock acquired; this is the rare path.
                    debug!(
                        "SmbVolume::write_from_stream: {} drained bytes ({} expected) don't go out as one frame; falling back to streaming writer",
                        buffer.len(),
                        size
                    );
                    let writer_result = open_file_writer(&tree, conn, &smb_path, mode).await;
                    let mut writer = self.handle_smb_result("write_from_stream(open)", &smb_path, writer_result)?;
```

- **Mechanism:**
  1. `stream_pipe_file` reads `size` off the source stream's `total_size()` (the local `stat` at open) and asks
     `write_is_single_shot(Known(size))`; SMB says yes when `size <= quick_write_limit` (`streams.rs:426`), so
     `resolve_staging` makes it `SingleShot(ExpectedFree)`: no `.cmdr-tmp-*`, no in-flight-temps record, `CreateNew`
     straight at the final name, stall watchdog disarmed (`stream_pipe.rs:287`).
  2. `LocalPosixReadStream::next_chunk` reads 1 MiB until `read()` returns 0, so a file appended to after the open
     yields more than `size`; no wrapper (`source_reads::watch`, `CheckpointStream`, `quarantine_stream`) caps it.
  3. The compound branch drains the WHOLE stream (`:531`). For the real name the limit is `max_write`
     (`one_frame_write_limit`, `:111`); a drained buffer past it fails `fits_one_compound_write` at `:553` and falls
     through to `open_file_writer(.., mode)` at `:620`: a multi-round-trip CREATE / WRITE / SET_INFO / CLOSE at the
     user's real name, exactly what the must-know in `crates/cmdr-smb/CLAUDE.md` ("❌ A refused one-frame write to a
     non-scratch name never streams") and the comment at `:512` forbid.
  4. A connection drop inside that window: `write_chunk` errors, `handle_smb_result` flips the volume to `Disconnected`,
     so `delete_partial` (`:491`) can't clone a session and does nothing. The partial stays at the real name.
  5. Engine side: `create_went_unanswered` is true, the outage wait ends `Back`, `abandon_attempt` deliberately leaves a
     `SingleShot` target alone (`staged_write.rs:429`), `settle_unanswered_create` sees a size mismatch and answers
     `RunAgain`, and the next attempt's `CreateNew` hits `AlreadyExists` on our own partial, which is reported as a
     taken name while the partial stays. A force-quit in the window leaves it too, and nothing sweeps it at launch (no
     temp was registered).
- **Trigger:** Copy (or move) a file that something is still writing (an app log, a recording, a download finishing, a
  database file) from the Mac to a NAS, where the file is near the one-frame limit at open and passes `max_write` while
  being read. On a LAN `quick_write_limit` rises to `max_write` itself (8 MiB on Samba and Windows, 64 KiB to 1 MiB on
  smaller NAS firmware, which the `smb-consumer-maxreadsize` fixture models), so a file a few KB under it growing a few
  KB is enough. Then Wi-Fi blips, the NAS sleeps, or the app is quit, in the next few round trips.
- **Guard ruled out:** The `CreditStarvation` guard (`:570`) covers only a refused frame, not an oversized buffer. The
  SMB download stream ends at its opened size (`streams.rs:328`), but the LOCAL source doesn't.
  `fits_one_compound_write` is re-checked on the drained length, which is what routes it here. `interrupted_landing.rs`
  only accepts an exact-size, exact-bytes match, so it never adopts or clears the partial. `remove_unstaged_partial` is
  off (`writing_at_the_final_name` is false for `SingleShot`).
- **Impact:** A truncated file at the user's real filename on the share, indistinguishable from a good copy by name,
  with the copy reporting a name clash. The source survives (a move's sweep only deletes ledgered successes), so it's
  recoverable once noticed; the danger is the user trusting the truncated copy, then deleting the original.
- **Suggested fix:** On the real-name branch, never stream: cap the drain at `size` and fail the write (a typed "source
  changed while reading" the engine can retry staged), or have the compound branch refuse a buffer that outgrew `size`
  before opening anything.

### smb-3. Signing back in to a disconnected share overwrites the saved server password before checking it, and saves it even with Remember off

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-smb/src/volume/reconnect.rs:272`, `do_reconnect_with_credentials`;
  `apps/desktop/src-tauri/src/commands/network.rs:640`, `reconnect_volume_with_credentials`;
  `apps/desktop/src/lib/servers/open-sign-in.ts:441`, `mendAttempt`
- **Excerpt:**

```rust
// crates/cmdr-smb/src/volume/reconnect.rs:272
    pub(super) async fn do_reconnect_with_credentials(
        &self,
        username: String,
        password: String,
    ) -> Result<(), VolumeError> {
        let server = { self.params.read().await.server.clone() };
        // Server-level (`scope: None`), so one password covers every share on it.
        let stored = StoredCredentials {
            username: username.clone(),
            secret: password.clone(),
        };
        if self
            .host
            .credentials()
            .save_credentials(&server, None, &stored)
            .is_err()
        {
```

- **Mechanism:**
  1. The pane's Sign in (after a `needs_credentials` give-up) calls `reconnect_volume_with_credentials` →
     `do_reconnect_with_credentials`.
  2. It writes `username` + `password` to the SERVER-level Keychain entry unconditionally, before any dial, then runs
     `do_attempt_reconnect`.
  3. A typo or a different account fails the dial, but the previous server-level entry (the one every share on that
     server falls back to, `refresh_credentials_from_store`) is already replaced. A different account succeeding still
     replaces the account the other shares on that server signed in with.
  4. The frontend's contract says the opposite: "It refreshes a remembered secret and never seeds one — that rule lives
     in the backend, which writes the store only where it already holds a secret for this account. The Remember box is
     what the user changes that with" (`open-sign-in.ts:441`). WebDAV, SFTP, and S3 follow it
     (`refresh_remembered_secret`, "❗ Refreshes, ❌ never seeds"); SMB seeds every time, so an unticked Remember box
     (which `withRememberFlip` has just acted on) is undone by the very next line.
- **Trigger:** The NAS turns a saved password away for a while (an AD controller blip, an account lockout another device
  caused, a password changed on another machine), Cmdr shows Sign in, and the user types a wrong password or signs in as
  someone else. Or the user unticks Remember on that sheet.
- **Guard ruled out:** `do_attempt_reconnect`'s re-pull reads the store AFTER the write, so it finds the bad entry;
  nothing saves the previous value. `connect_directly_with_credentials` (`smb_connect_directly.rs:289`) saves only after
  success and only when `remember_in_keychain`, so the defect is this path alone.
- **Impact:** The user's saved SMB password (a secret, per the brief) is replaced by an unverified one or a different
  account's, for every share on the server; recoverable only if they still know it. Remember-off is silently ignored,
  keeping a secret the user asked not to keep.
- **Suggested fix:** Follow the sibling backends: dial with the typed credentials first, then
  `refresh_remembered_secret` (write only where an entry for this account already exists, and only after success).

### smb-4. A saved-shares or saved-servers file the app can't parse is replaced by an empty one on the next save

- **Severity:** low
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/network/known_shares.rs:174`, `load_known_shares`;
  `apps/desktop/src-tauri/src/network/manual_servers.rs:458`, `read_store_from_path`
- **Excerpt:**

```rust
// apps/desktop/src-tauri/src/network/known_shares.rs:174
pub fn load_known_shares<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(path) = get_store_path(app) else {
        return;
    };
    let _ = STORE_PATH.set(path.clone());

    cleanup_tmp_file(&path);

    let store = if let Ok(contents) = fs::read_to_string(&path) {
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        KnownSharesStore::default()
    };

    *get_known_shares_mutex().lock_ignore_poison() = store;
}
```

- **Mechanism:** Any parse failure (one unknown `ConnectionMode` / `AuthOptions` variant, both strict enums with no
  `#[serde(other)]`, written by a newer build) becomes an empty in-memory store; the next `remember_mount`, pin, or move
  writes that empty store over the file (`save_known_shares`, atomic). `manual_servers.rs` does the same with the
  manual-server list, which holds typed usernames and per-server switches.
- **Trigger:** Running an older Cmdr after a newer one wrote the file (reinstalling a previous beta, or a beta channel
  switch).
- **Guard ruled out:** Writes are temp + rename, so torn files aren't the risk; there's no backup, no schema version,
  and no refusal to save over a file that failed to parse. `server_list_file.rs:3` documents "losing the file costs one
  re-entry" as deliberate for the SFTP/WebDAV lists, but these two also hold pins, the per-share direct-connection
  switch, and pending server moves (`known_shares::pending_moves`), which a re-entry doesn't restore.
- **Impact:** Lost saved shares, pins, and pending-move records (Cmdr state; passwords stay in the Keychain).
  Recoverable by hand.
- **Suggested fix:** On a parse failure, move the file aside (`known-shares.json.unreadable`) before the first save, or
  keep the bad file and refuse to save until it's read.

## Near misses

- Forced rename onto a file (`mutation.rs:184`): one `ReplaceIfExists` SET_INFO; only a directory at either end falls
  back to clear-then-rename, and a case-variant of the source is never cleared. A FOLDER forced onto a FILE loses the
  file if the follow-up plain rename then fails; not chased whether the UI ever forces a cross-type rename.
- NFC/NFD look-alike forced rename on a normalization-insensitive server (macOS smbd on APFS): `same_name_ignoring_case`
  folds case only, so it could clear the source, but `delete_directory` only removes an EMPTY one. Worst case an empty
  folder.
- Compound single-shot write (`streams.rs:564`): `CreateNew` sends `FileCreate`, a CREATE refusal is never cleaned up
  (`create_succeeded_but_write_failed`), so another writer's file is safe.
- `CreateOrReplace` single-shot only reaches a name the caller claimed (`resolve_staging`); a real overwrite goes
  through the caller's `AlreadyStaged` safe-replace.
- `copy_on_server` (`copy.rs`): `refuses_occupied()` modes answer `NotSupported`, and `try_server_side_copy` always
  stages, so the truncate-in-place of `open_copy_destination` only ever hits a `.cmdr-tmp-*`.
- Streamed writes delete their partial on error; when the session is gone the temp is a registered `.cmdr-tmp-*`, swept
  at launch.
- `\` and `/` in server names: smb2 maps a name's `\` to U+F026 and only `/` separates, so a listed name can't address
  another path. `.`/`..` entries are filtered (`query.rs:81`).
- A share path containing `..` goes to the server unrejected (`paths.rs:35`); Windows and Samba refuse `..` components,
  so no escape found.
- Recursive SMB delete on Samba with `follow symlinks = yes`: the server presents an in-share symlink as a plain folder,
  so recursing is the server's semantics (Finder and Explorer do the same); not a Cmdr defect.
- Server-move secrets (`server_move_smb.rs:96`): one-key spellings skip the copy, so the delete can't take the only
  password; copies before move, deletes last.
- `SmbCredentials` derives `Debug` with the plaintext password (`keychain.rs:20`), but no log site formats one;
  `SmbConnectionParams` masks it.
- `smbclient -A` file: `create_new` + `0o600`, unlinked on drop. `gio mount` gets the password on stdin; smbutil URLs
  are passwordless.
- `at_active_root` defaults an unrecorded root's anchor to `""` (`mod.rs:400`), but `instance_at_root` only records an
  unrecorded root when the anchor is already `""`, so they agree.
- `unmount_smb_shares_from_host` (`mount.rs:691`) runs `diskutil` with no deadline, against the `output_within`
  must-know: a hang, not a data risk.

## Covered

Read in full:

- `crates/cmdr-smb/CLAUDE.md`
- `crates/cmdr-smb/src/volume/mutation.rs`, `streams.rs`, `volume_impl.rs`, `copy.rs`, `mapping.rs`, `scan.rs`,
  `session.rs`, `reconnect.rs`, `paths.rs`, `spelling.rs`, `query.rs`, `attributes.rs`, `stamping.rs`, `mod.rs` (lines
  95–580)
- `apps/desktop/src-tauri/src/server_move.rs`, `server_move_smb.rs`
- `apps/desktop/src-tauri/src/network/CLAUDE.md`, `mount.rs`, `keychain.rs`, `smb_smbclient.rs`, `server_list_file.rs`
- `apps/desktop/src-tauri/src/network/smb_connect_directly.rs` (lines 200–404)

Followed out of the slice (partial reads):

- smb2 `v0.31.1` `tree.rs` (`delete_file`, `rename_with`, `delete_directory`, `delete_compound`, `DirectoryEntry`)
- `crates/cmdr-operations/src/delete/walker.rs:330–470`
- `transfer/staged_write.rs`, `transfer/volume/stream_pipe.rs:180–450`, `transfer/volume/server_side_copy.rs`,
  `transfer/interrupted_landing.rs`, `transfer/volume/source_reads.rs`
- `crates/cmdr-local/src/local_posix/streams.rs`
- `crates/cmdr-sftp/src/volume/reconnect.rs:130–200`
- `apps/desktop/src-tauri/src/commands/network.rs` (credential and reconnect commands)
- `apps/desktop/src/lib/servers/open-sign-in.ts:425–475`
- `known_shares.rs:1–215`, `manual_servers.rs:450–475`, `mount_linux.rs` (credential lines), `smb_smbutil.rs:187–201`

## Not covered

- `crates/cmdr-smb/DETAILS.md`, `src/{lib,types,errors,connection}.rs`, and
  `src/volume/{watcher,scan_pool,random_reader,credit_cap_proxy,foreground_yield,liveness,slow_calls,state}.rs`. These
  are read-side or cache-side; the watcher only feeds the listing cache, which the delete walker reads as an oracle (a
  missed entry fails the final `rmdir`, the safe direction).
- In `network/`: `DETAILS.md` and most of the remaining ~40 files (discovery, mDNS, share listing, upgrade and
  pane-upgrade flows, SFTP/WebDAV/S3 wiring and stores, `one_shot_credentials.rs`, `credential_store.rs` beyond
  `save_credentials`, `smb_saved_shares.rs`, `manual_servers*.rs` beyond the store load, `mount_linux.rs` beyond its
  credential path).
- The `cmdr-smb` integration tests. No live server was run (the hunter rules forbid it), so smb-1's server-side junction
  resolution is unverified empirically.
