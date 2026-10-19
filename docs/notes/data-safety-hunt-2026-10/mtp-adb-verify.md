# Data-safety hunt 2026-10: MTP and ADB verification (`mtp-adb`)

Verifier's pass over `mtp-adb-hunt.md`, read-only at `22918588b`. Every cited line and excerpt was checked against the
tree and matches verbatim.

### mtp-adb-1

- **Verdict:** **duplicate of delete-1** for the walker half; the ADB-specific half is **confirmed**.
- **Severity:** high for the ADB half (delete-1 carries the walker at critical; the ADB half adds nothing beyond it
  except the link-survives ending).
- **Evidence:**
  - The ADB listing reports a link to a folder as `is_directory: true` AND `is_symlink: true` (not `is_symlink: false`):
    `stat_to_file_entry` sets `is_symlink` from the `lstat` kind (`crates/cmdr-adb/src/volume/mapping.rs:18`), then
    `list_directory_impl` follows each link with `readlink -f` + `STAT` and `with_link_target` only flips `is_directory`
    (`mapping.rs:31`, `query.rs:102-115`). `get_metadata_impl` does the same for one path (`query.rs:139-143`). So the
    defect is the walker ignoring `is_symlink`, not ADB hiding the link.
  - `AdbVolume` doesn't override `is_directory` or `entry_kind` (`volume_impl.rs` has only `delete`/`rename` among the
    relevant ones), so the trait default `get_metadata().is_directory` (`crates/cmdr-fs/src/volume/mod.rs:319-324`)
    reads a selected link as a folder at `walker.rs:350-353`, and children recurse on `child.is_directory` alone at
    `walker.rs:440-442`.
  - Children of the link are built under the link's path (`query.rs:73`, `join_device_path(&device, &entry.name)`), and
    the sync `LIST` opens the path with `opendir`, which follows the link. `delete_impl` then runs `rm -f` / `rmdir` on
    `…/link/child` (`writes.rs:136-155`), which the kernel resolves through the link.
  - The final `delete(link)`: `probe` follows the link to `Directory` (`writes.rs:256-259`), `rmdir` of a symlink fails,
    the re-listing is now empty, so the verb's error returns and the link survives. As described.
  - No guard found: `can_watch_listings` is false, so `authoritative_listing` never supplies a cached child list for
    ADB.

### mtp-adb-2

- **Verdict:** **plausible**, but only on a device that breaks the MTP spec.
- **Severity:** medium (hunter: high). The loss is real if it happens, but a spec-compliant device can't produce it, and
  it also needs a pane row to stay stale on screen.
- **Evidence:**
  - The code path is as described: `resolve_path_to_handle` answers any cached forward entry with no device check
    (`crates/cmdr-mtp/src/connection/resolve.rs:90-92`); `finalize_listing` only inserts (`directory_ops.rs:692-702`),
    so a name gone from a fresh listing keeps its forward entry; `ObjectRemoved` refreshes panes and the index but never
    touches `path_cache` (`event_loop.rs:145-149`). `delete_object_with_cancel` fetches `object_info` and never compares
    `filename`/`parent` with `object_path` (`mutation_ops.rs:100-107`); `rename_object` (`:330`) and `move_object`
    (`:416`) resolve the same way.
  - **The MTP spec forbids the reuse the finding needs.** MTP 1.1 § 3.4.1
    (`~/projects-git/vdavid/mtp-v1_1-spec-md/04-normative-reference.md:214`): "If an object is deleted in a session, the
    Responder shall not re-use the deleted object's Object Handle in the same session." § 3.4 adds that handles are only
    valid within a session.
  - **Session boundaries are covered.** `mtp-rs` never reopens a session behind the caller's back: on a wedged cancel it
    returns `DeviceReset` and leaves the reopen to the consumer
    (`mtp-rs/crates/mtp-rs/src/transport/nusb.rs:1013-1024`). Cmdr's `handle_device_session_reset` drops the whole
    `DeviceEntry`, path cache included (`connection/DETAILS.md` § session reset, step 1, pinned by
    `session_reset_drops_the_dead_session_and_its_handle_caches` in `session_reset.rs`), and a disconnect drops it too.
  - **The crate's own "devices reuse handles" claims are weaker than they read.** `connection/DETAILS.md:230` is about
    Android handles going STALE across a media rescan (invalid, refused with `InvalidObjectHandle`), which the hunter
    already concedes is the safe case. The "MTP devices REUSE object handles" comments (`cache.rs` `remove_path`,
    `connection/DETAILS.md:287,409`) are true across sessions, and of non-compliant firmware: hathach/tinyusb#4045
    reports the TinyUSB MTP example reissuing a freed handle within a session. Modern Android's `MtpStorageManager`
    hands out handles from an incrementing counter per session (from memory of AOSP, not re-read here), so the phone
    case (most of Cmdr's MTP users) shouldn't hit it.
  - What survives: a non-compliant camera or embedded device that reuses handles within a session AND sends no events,
    with a stale row left on screen. The fix the hunter suggests (compare `object_info.filename` and `parent` before a
    destructive call) is cheap and also hardens against Cmdr's own stale-forward-entry leak, so it's still worth doing.

### mtp-adb-3

- **Verdict:** **confirmed**.
- **Severity:** medium (agree).
- **Evidence:**
  - `entry_for_object` builds `child_path = parent_path.join(&info.filename)` with no disambiguation
    (`crates/cmdr-mtp/src/connection/directory_ops.rs:880-901`), and `PathHandleCache::insert` keys the forward map on
    that path (`cache.rs:37-40`), so twins collapse to the last-listed handle at `directory_ops.rs:697-699`. No
    duplicate-name detection anywhere in `directory_ops.rs` / `file_ops.rs`.
  - Every path-addressed op resolves through that map (`file_ops.rs:93,188`, `mutation_ops.rs:100,330,416`).
  - PTP permits same-name siblings (`crates/cmdr-mtp/DETAILS.md:125`), and Cmdr itself can create them on a permissive
    device with a `force` rename (hunter's near-miss list).
  - Not silent in the copy case: the second row lands on the same destination name, so the user sees a conflict prompt.
    That's why medium, not high; the delete-the-wrong-twin case is silent but the loss is of a same-named file in the
    same folder.

### mtp-adb-4

- **Verdict:** **confirmed**.
- **Severity:** medium (agree), with a nuance: in the plain case the loss is SMALLER than what the user consented to.
- **Evidence:**
  - Exact invocation: `shell_verb(&["mv", "-f", &device_from, &device_to], …)`
    (`crates/cmdr-adb/src/volume/writes.rs:170`), sent through `shell::run`, which single-quotes each argument
    (`shell.rs:56-72`). For the trigger that is `mv -f '/sdcard/Download/notes.txt' '/sdcard/Download/Backup'`, with no
    `-T`, so `mv` treats an existing directory `Backup` (or a link to one) as the target directory and lands
    `Backup/notes.txt`, `-f` replacing what's there.
  - The path reaches it: `handleConflictResolve` `'overwrite-delete'` calls `performRename(…, true, …)`
    (`apps/desktop/src/lib/file-explorer/pane/rename-flow.svelte.ts:697-700`); the conflict dialog opens for any taken
    name with no kind check (`rename-flow.svelte.ts:414-423`, `RenameValidityResult` carries no kind refusal,
    `crates/cmdr-operations/src/rename/validity.rs:23-38`); `rename_target` lets `Taken` through with `force`
    (`crates/cmdr-operations/src/rename.rs:405`) and the volume branch calls `volume.rename(&from, &to, force)`
    (`rename.rs:252`). `rename_impl` with `force` skips every destination check (`writes.rs:164`).
  - Nuance: the user confirmed replacing ALL of `Backup`, so losing `Backup/notes.txt` is a subset of the consented
    loss; the real harm is the wrong result (item buried in the folder, pane patched as if `Backup` was replaced) plus
    the destination-is-a-link-to-a-folder case, where the replaced file lives outside what the user was shown.

### mtp-adb-5

- **Verdict:** **confirmed**.
- **Severity:** low (agree).
- **Evidence:**
  - `probe` maps an `open_sync` failure and any `stat` `Err` to `WhatIsThere::Nothing` (`writes.rs:250-266`, excerpt
    verbatim), and a v2 `STAT` with an errno also reads as not-existing (`sync.rs:66-68`,
    `exists() = errno.is_none() && mode != 0`).
  - `rename_impl(force = false)` gates only on that probe (`writes.rs:164`), and the `CreateNew` landing gates on it
    right before `mv -f` (`streams.rs:189-193`), with no second probe.
  - One more trigger worth noting beyond flaky transport: the sync service runs in `adbd`'s SELinux domain while `mv`
    runs as the shell user, so a path the sync `STAT` is denied on (errno `EACCES`) but the shell can write would make
    the guard read "nothing there" deterministically. Not traced on a device.
