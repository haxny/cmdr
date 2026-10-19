# Data-safety hunt 2026-10

The second adversarial data-safety hunt (GitHub issue #147), run on 2026-10-19 against `22918588b`. The first hunt
(2026-09-01) covered only the copy/move transfer engines; this one covers everything else that writes, deletes, or
persists user data. Nothing here is a fix: it's a ranked triage list for principle #1 (protect the user's data), with
one GitHub issue (#505–#548) per distinct defect.

## Method

- The code was cut into 13 slices. Each got one hunter, then one verifier, both read-only (no builds, no app runs),
  working from `brief.md`: a strict per-finding schema (severity, confidence, `file:line`, verbatim excerpt, mechanism,
  trigger, guard ruled out, impact, suggested fix), at most eight findings, each one attacked before it's reported.
- Slices: `archive`, `delete` (delete, trash, clipboard), `volume` (volume layer and local FS), `smb`, `sftp-webdav`,
  `mtp-adb`, `s3`, `oplog` (operation log and rollback), `settings` (settings and secrets persistence), `viewer`,
  `index-store`, `index-scan` (with the media index and `fsevent-stream`), and `misc` (git, Downloads, listing, cloud,
  tags, open with, rename, create). S3, WebDAV, and ADB postdate the original scope list in #147 and were added.
- The verifier's verdict and severity win over the hunter's. Refuted findings are dropped (none were). Duplicates across
  slices are merged into one line below, with every source id named.
- Per slice: `archive-hunt.md` / `archive-verify.md`, `delete-hunt.md` / `delete-verify.md`, `volume-hunt.md` /
  `volume-verify.md`, `smb-hunt.md` / `smb-verify.md`, `sftp-webdav-hunt.md` / `sftp-webdav-verify.md`,
  `mtp-adb-hunt.md` / `mtp-adb-verify.md`, `s3-hunt.md` / `s3-verify.md`, `oplog-hunt.md` / `oplog-verify.md`,
  `settings-hunt.md` / `settings-verify.md`, `viewer-hunt.md` / `viewer-verify.md`, `index-store-hunt.md` /
  `index-store-verify.md`, `index-scan-hunt.md` / `index-scan-verify.md`, and `misc-hunt.md` / `misc-verify.md`.

## Severity scale

The hunt's scale (from `brief.md`):

- **critical**: silent permanent loss of user data in a realistic flow.
- **high**: loss or corruption needing an unusual but plausible condition, or loss the user is told about but can't
  recover.
- **medium**: wrong result, wedged state, or recoverable loss.
- **low**: hygiene.

The board's Severity field (`docs/tooling/backlog-board.md`) has a different scale, where Critical means any data loss.
The issues map hunt critical to Critical, high to High, medium to Medium, and low to Low; none of the highs is both
silent-permanent and in a common flow, so none was lifted to Critical.

## The ranked list

44 issues: one critical, 12 high, 20 medium, and 11 low. Each line: severity and verdict, what goes wrong for the user,
the source ids, and the issue.

1. **critical, confirmed**: Deleting a folder on an external drive, cloud drive, ADB phone, or SFTP server (for a
   selected link) also deletes everything its symlinks to folders point at. `delete-1` (= `misc-1`, `mtp-adb-1`,
   `sftp-webdav-1`). #505.
2. **high, confirmed**: An unreadable `settings.json`, `shortcuts.json`, `app-status.json`, or `license.json` is
   replaced with an empty store on quit. `settings-1`. #506.
3. **high, confirmed**: A permanent delete of a folder that holds a mount point empties the mounted volume. `delete-3`.
   #507.
4. **high, plausible**: Delete, cut, copy, and drag resolve stale row indices and can act on a neighboring file when the
   folder changes at that moment. `delete-2`. #508.
5. **high, confirmed**: Moving a folder into a zip that holds a same-named file hides its files in the zip and deletes
   them from disk. `archive-2`. #509.
6. **high, confirmed**: Deleting or moving a markerless S3 folder also deletes a hidden same-named file beside it.
   `s3-1`. #510.
7. **high, confirmed**: Undoing a case-only rename on a case-sensitive volume overwrites the file that now holds the old
   name. `oplog-1`. #511.
8. **high, confirmed**: Renaming onto an existing folder's name with Overwrite deletes the whole folder on WebDAV and
   buries the item inside it on ADB. `sftp-webdav-2` + `mtp-adb-4` (medium). #512.
9. **high, confirmed**: A remote zip edit whose final rename fails leaves the only copy under a `.cmdr-tmp-` name, which
   the transfer reaper deletes an hour later. `archive-1`. #513.
10. **high, confirmed**: Any edit of a zip with non-UTF-8 names rewrites every non-ASCII name as CP437 mojibake.
    `archive-3`. #514.
11. **high, confirmed**: Edits an app saves into an Open with copy of a zip entry are deleted at the next Cmdr start.
    `viewer-1` (verifier raised it from medium). #515.
12. **high, plausible**: Deleting a folder on a Windows share that holds a junction deletes the junction's target.
    Depends on #1 and an smb2 change. `smb-1`. #516.
13. **high, confirmed**: Favorites, rename presets, histories, and saved S3/SFTP/WebDAV/SMB servers are wiped when their
    file can't be read or parsed (one umbrella, per-site severities high to low). `settings-2` (verifier raised it from
    medium) + `s3-3` + `smb-4`. #517.
14. **medium, confirmed**: Rolling back a copy whose original is gone permanently deletes the only copy. `oplog-6`.
    #518.
15. **medium, confirmed**: Rolling back a copy to a non-local volume deletes a same-size file edited since. `oplog-5`.
    #519.
16. **medium, confirmed**: A small file that grows while copied to SMB streams to its real name, so a drop leaves a
    partial there. `smb-2` (verifier lowered it from high). #520.
17. **medium, plausible**: A WebDAV folder delete can take a file that arrived between the emptiness check and the
    recursive `DELETE`. `sftp-webdav-3`. #521.
18. **medium, plausible**: A cut-off PUT to an "Other" S3 server can leave a truncated object under the real name.
    `s3-2`. #522.
19. **medium, confirmed**: Two same-named files in one MTP folder resolve to one object, so a backup misses one and a
    delete can hit the other. `mtp-adb-3`. #523.
20. **medium, plausible**: MTP destructive ops trust a cached handle without checking its name, so a device that reuses
    handles gets the wrong object deleted. `mtp-adb-2` (verifier lowered it from high). #524.
21. **medium, confirmed**: Moving a folder out of a malformed zip deletes hidden entries under it that never landed.
    `archive-4`. #525.
22. **medium, plausible**: An encrypted entry after a duplicate name decrypts as the next entry, and a move then deletes
    the real one. `archive-5`. #526.
23. **medium, confirmed**: The operation log folds folder names by case, so a rollback on a case-sensitive volume acts
    on a case-twin folder. `oplog-2` (verifier lowered it from high). #527.
24. **medium, confirmed**: The replace-a-folder prompt shows the index's lower-bound or stale folder size as exact; the
    delete dialog's per-row sizes too. `index-store-1` (= `index-scan-1`) + `index-store-2` (low). #528.
25. **medium, confirmed**: F2 or multi-rename on the `.git` portal's `worktrees` row renames git's real folder, and MCP
    can rename a worktree's admin folder. `volume-1` + `volume-v1` (verifier-found). Relates to #463. #529.
26. **medium, confirmed**: Signing back in saves the typed password before the dial checks it; SMB also saves with
    Remember off. `smb-3` + `sftp-webdav-5` (low). #530.
27. **medium, confirmed**: A wildcard `@revoked` host key line in `known_hosts` is ignored. `sftp-webdav-4`. #531.
28. **medium, confirmed**: Image indexing reads through symlinks, so an excluded folder's OCR text reaches search and
    agent tools. `index-scan-2`. #532.
29. **medium, confirmed**: "Finish rollback" never retries skipped items, then marks the op fully rolled back.
    `oplog-3`. #533.
30. **medium, confirmed**: A journal page read error ends a rollback as fully rolled back. `oplog-4`. #534.
31. **medium, plausible**: Undoing a trash can restore a different folder that later took the same name in the Trash.
    `delete-4`. #535.
32. **medium, plausible**: A setting changed just before Settings opens is deleted from `settings.json` when Settings
    closes. `settings-3`. #536.
33. **medium, confirmed**: On Linux without a keyring, a changed machine id wedges `credentials.enc` so nothing can be
    saved again. `settings-4`. #537.
34. **low, confirmed**: The transfer engine never checks delivered bytes against the declared length before committing
    (and before a move deletes its source); an engine gap the first hunt missed. `s3-4` + `sftp-webdav-6` +
    `sftp-webdav-7`. #538.
35. **low, confirmed**: An ADB no-clobber probe reads "couldn't ask" as "nothing there", so `mv -f` can clobber.
    `mtp-adb-5`. #539.
36. **low, confirmed**: A WebDAV server saved as `http://` sends the password in clear text with no warning.
    `sftp-webdav-8`. #540.
37. **low, confirmed**: Toggling a color tag on a file with unreadable tags replaces all its tags. `misc-2`. #541.
38. **low, confirmed**: A zip rewrite drops the comment, any prefix, and a duplicate's earlier copy, and replaces a
    symlink to the zip with a file. `archive-6` + `archive-7`. #542.
39. **low, confirmed**: The viewer's "Save as file" writes U+FFFD for undecodable bytes, and can rename into place after
    reporting a timeout. `viewer-3` + `viewer-4`. #543.
40. **low, confirmed**: Open with copies pile up for the whole session, up to 2 GiB each. `viewer-2`. #544.
41. **low, confirmed**: S3 upload-ledger records orphan after an endpoint edit, so the parts stay billed. `s3-5`. #545.
42. **low, confirmed**: On Linux, the multi-rename Results file is a predictable `/tmp` path written through symlinks.
    `misc-3`. #546.
43. **low, confirmed**: A FIFO or device link named like an image hangs the Vision worker. `index-scan-3`. #547.
44. **low, plausible**: An FSEvents event with an unknown flag bit is dropped without setting overflow. `index-scan-4`.
    #548.

Related open issues these link (none is a duplicate): #463 (routed renames, from #529), #116 (an in-app clipboard would
add callers of the same selection resolution, from #508), #338 (the index's own cut at mounts, from #507), #258 (the zip
rewrite path, from #514 and #542), and #260 (the remote zip edit round trip, from #513).

## Coverage gaps

Each slice's own "Not covered" list, condensed. The two index slices and the `agent/` subtree left large areas unread;
treat them as unhunted, not as clean.

- **index-scan**: about 40,000 lines of non-test Rust, read by targeting only the paths that reach user files or a
  destructive decision. Not read: `indexing/reconcile/**`, `indexing/watch/event_loop/**`, `watch/branches.rs`,
  `scanner/mod.rs`, `walker/engine.rs`, `network_scanner/**`, `transports/**`, `media_index/scheduler/` beyond
  `enrich.rs`, `media_index/store`, `ann`, `vector`, `coverage`, and `fsevent-stream`'s `ffi.rs`, `observer.rs`,
  `lib.rs`. These write only Cmdr's own databases (medium at most), but rename-by-inode correctness and media GC on a
  partial index are worth a dedicated hunt.
- **index-store**: all of `indexing/writer/`, `indexing/aggregator/`, most of `indexing/store/` and
  `indexing/lifecycle/`, `read/enrichment.rs`, `coverage.rs`, `pending_sizes.rs`, `indexing/host/`, `deletes.rs`,
  `hold.rs`, and most of `listing_index_sizes/`.
- **settings**: `agent/` beyond `agent/memory/` (about 31,000 lines: `store/` with `main.db`, `chat/`, `tools/`,
  `wake/`, `suggested_ops/`) was out of reach; `AgentStore::open`'s corrupt-file recovery is untraced. Also skipped:
  `settings/loader.rs`, `ai_gates.rs`, `restricted_shortcuts.rs`, licensing trust logic, `keyring_linux.rs`,
  `system_keychain_smb.rs`, most of `keychain_macos.rs`, and the frontend settings registry and applier.
- **smb**: `cmdr-smb/DETAILS.md`, the read-side and cache modules (`watcher`, `scan_pool`, `random_reader`, `liveness`,
  and others), about 40 files of `network/` (discovery, mDNS, upgrade flows, the SFTP/WebDAV/S3 wiring and stores,
  `credential_store.rs`), and the integration tests. No live server was run, so `smb-1`'s junction behavior is
  doc-based.
- **sftp-webdav**: SFTP `auth.rs`, `errors.rs`, `extensions.rs`, `params.rs`, `volume/state.rs`, most of `transport.rs`
  (dial, lifecycle); WebDAV `errors.rs`, `propfind.rs`, `volume/state.rs`, `volume_impl.rs`; and the desktop's
  `sftp_volume_wiring.rs`, known-servers stores, `live_server_edit.rs`, `one_shot_credentials.rs`.
- **mtp-adb**: ADB's wire codecs and lifecycle (`transport`, `sync`, `devices`, `server`, `features`, `errors`,
  `params`, `fuzzing`, `state`, `index_scope`); most of `cmdr-mtp/src/connection/` beyond the mutation, resolve, cache,
  and directory paths; `adb/device_provider.rs` and `mtp/` desktop wiring; `mtp-rs` itself.
- **s3**: `upload_body.rs`, `scan.rs`, `routing.rs`, `profile.rs`, `refusal.rs`, `params.rs`, `multipart.rs`,
  `metadata.rs`, `xml/parse.rs`, `cost/*`, most of `transport/mod.rs`, `sigv4.rs` signing, `s3_costs/plan.rs`, and the
  rest of `s3_known_places.rs` (relocate, forget, migrate). Whether a pane relists after an S3 volume swap (`s3-1`'s
  second trigger) is untraced.
- **archive**: the fresh-compress pipeline (`fresh_plan.rs`, `fresh_zip.rs`, `fresh_pipeline.rs`, `fresh_validate.rs`,
  `fresh_rows.rs`), `conflicts.rs`, the tar and 7z readers, `extract.rs`, `watch/`, `boundary.rs`, `volume_streams.rs`,
  `volume_scan.rs`, `file_viewer/materialize.rs`, and the MCP archive tools.
- **oplog**: `query.rs`, `types.rs`, `recording.rs`, `rollback/order.rs`, `skips.rs`, `runner.rs`, the writer's message
  types, `journal_search.rs`, and the cross-FS move's `JournalDestUnder` rebase.
- **delete**: clipboard `payload.rs` and `mock.rs`, the content-paste command, volume eject (`drive_release/`,
  `unmount_approver/`, `server_move_smb.rs`), and the MTP delete and SMB listing in full.
- **volume**: `cmdr-local`'s read-only probes (`reading.rs`, `space.rs`, `mount_table.rs`, `linux_mounts.rs`, and
  others), `cmdr-fs` test oracles and several helpers (`scan_walk.rs`, `patching.rs`, `canonical_root.rs`,
  `name_fold.rs`), and most of `file_system/volume/eject/`.
- **viewer**: the read-only backends (`byte_seek.rs`, `line_index.rs`, `rows.rs`, the media backends, `watcher.rs`, and
  others), `quick_look/`, and the viewer route's rendering. A grep showed no other write sites.
- **misc**: most of `cmdr-git` (read-only by type), `downloads/` beyond its docs, `cmdr-listing` beyond
  `authoritative.rs`, `file_system/watcher.rs`, `listing_host/`, the File Provider ObjC side, `google_drive/`, open
  with's launch code, and most of `multi_rename/`.

## Method notes, for the next run

- **The strict schema paid off.** A verbatim excerpt plus a `file:line` per finding let every verifier check the claim
  in minutes, and "guard ruled out" forced hunters to kill their own weak candidates first; the near-miss lists are
  worth reading before re-hunting a slice.
- **Cross-slice messaging found duplicates fast.** Hunters told each other when a mechanism crossed into another slice
  (`mtp-adb` handed `archive-1` and the walker half of `mtp-adb-1` over), so merging here was mostly bookkeeping.
- **Three hunters found the critical one independently.** `delete`, `misc`, and `mtp-adb` each reported the symlink
  walk, and `sftp-webdav` found its SFTP top-level variant, which the delete verifier had wrongly called safe. Seams
  between slices (here, the listing's `FileEntry` shape feeding the delete walker) deserve their own attention.
- **Verifiers changed severities both ways.** Raised: `settings-2` and `viewer-1` (medium to high). Lowered: `smb-2`,
  `mtp-adb-2`, and `oplog-2` (high to medium). One verifier found a new variant (`volume-v1`). Keep the verify pass;
  hunter severities alone would have misranked five items.
- **Not every slice was equal.** The index slices and `agent/` were too big for one hunter each; split them next time.
