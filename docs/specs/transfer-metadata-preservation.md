# Transfer metadata preservation (plan)

**Problem.** A copy or move in Cmdr keeps a file's bytes and its modification date, and quietly drops much of the rest:
creation dates, Finder tags and comments on folders, a folder's permissions, Locked and Hidden flags, and, across most
backends, everything but mtime. A move makes the loss permanent, since the original is deleted. A user asked for
"original timestamps, similar to `ditto`" (Lars, macOS 15.8.1, Cmdr 0.51.0); David's goal is wider: **no more lost
metadata**. `ditto` parity is the floor, not the ceiling, and our own measurements show `ditto` itself drops creation
dates across volumes (`docs/notes/transfer-metadata-experiments-2026-10-11.md`).

**Status**: M0 (characterization) done, M1 next (2026-10-11); review rounds 1 (correctness and data safety), 2
(architecture, resource cost, executability, Linux, SMB), and 3 (coherence, the M5a seam traced against the code,
fixture verifiability, M0/M1 agent-readiness) folded in. Issue: none yet. Evidence: the notes file above, plus the
file:line citations in § "Fresh read". Sizes are rough agent-sessions (one session ≈ one focused milestone agent run).

## Loud rules

1. ❌ **Never fail a copy over metadata.** The bytes are the copy. Every metadata step is best effort: a refusal is
   counted (§ "Telling the user") and logged, never an error. The one exception is the landing itself (rule 3).
   **Enforced by the type, not by convention**: the shared helper (§ "Design sketch") returns a `MetadataReport` and has
   no error path, and each class is its own independent step. Today's `copy_metadata` chains its steps with `?`
   (`T/chunked_copy.rs:280-294`), so a refused time-set silently skips the `chmod` after it; ❌ never chain classes.
2. ❌ **Dates go after everything that can bump mtime, in one call.** Set birth and mtime (and atime where we set it) in
   ONE `setattrlist` / `File::set_times`, after every xattr. A resource-fork write bumps mtime, and on APFS/HFS+ an
   mtime earlier than the birth time drags birth down to it (verified, notes § "The quirks"). ACLs and flags may follow
   the dates, because neither bumps mtime (verified). **xattrs go strictly before mode**: a path-based xattr write on a
   `0444` file is `EACCES` (verified).
3. ❌ **Nothing that blocks a rename or a later step goes on an item that will still be renamed.** That's the staged
   `.cmdr-tmp-*` temp, AND every item of a cross-FS move's staging tree until Phase 3 has renamed it into place.
   - Rename-blockers (all verified): `uchg`/`schg`/`uappnd` on the item; an ACL with `deny delete` on the item or
     `deny delete_child` on its folder; `uchg` on the folder it leaves; and, for a FOLDER moving to another parent, a
     mode without owner write (`0500`: `EACCES`).
   - They apply to the LANDED item: ACL first, then flags (`uchg` blocks a later ACL write and a later time-set,
     verified). Mode (files), xattrs, and dates go on the temp, as today. This also keeps every `.cmdr-tmp-*` removable
     by the sweep.
   - The one window this rule can't close: `clonefile` copies the source's flags onto the temp in the same call
     (verified), so a crash between the clone and Cmdr clearing them leaves a `uchg` temp. Rule 9 covers it: the
     in-flight sweep clears a temp it has already matched to its record before removing it.
4. ❌ **Only metadata of things the copy CREATED.** A folder the copy merged into keeps its own metadata (today's rule
   for dates, `volume/DETAILS.md` § "Copies keep the source's date"). An Overwrite replaces the file, so the landed file
   wears the SOURCE's metadata, the way Finder's Replace does.
5. ❌ **Never widen, never escalate, not even mid-copy.** Mode follows the `landed_mode` fold (umask-respecting) for
   routed volumes; setuid, setgid, and sticky travel only local-to-local; `SF_*` system flags and ownership never travel
   (non-root can't, and we don't want to when elevated either, unless a later spec decides so). A temp is created
   owner-only (`0600`) and set to its final mode at the end, so a `0600` source is never readable by others while its
   bytes stream, and a refused `chmod` leaves it narrower, never wider. (Today the chunked temp is created `0644`.) ❗
   The `landed_mode` fold reads the temp's CREATED mode as its umask proxy (`landed_mode.rs:46-60`), so on the volume
   path a `0600` temp would fold every file down to `0600`. Wherever the fold runs, pass it the umask-derived default
   explicitly (`0o666 & !umask`, read once per process), never the temp's mode.
6. ❌ **No new per-file round trips on remote sources, and no new syscalls for an ordinary file.** Metadata rides what
   the open or the listing already returned (the `modified_at` discipline). A class that would cost a request per file
   is gated on a capability, like `reports_posix_mode` gates the mode probe today. Locally, every class beyond today's
   is CONDITIONAL on the source having a value (no flags → no `chflags`, no ACL → no ACL write), so a plain file costs
   what it costs today (budget in § "Resource cost").
7. ❌ **Don't touch the staging temp NAMING.** A parallel effort is changing its length
   (`crates/cmdr-fs/src/staging.rs`, `volume/finalize.rs`). This plan only changes what goes on the temp and when.
8. ❌ **No FAT or exFAT images in tests.** `cmdr_fs::testing::disk_images` supports APFS and HFS+ only, for a
   kernel-panic reason (`crates/cmdr-index/src/indexing/tests/CLAUDE.md`). FAT/exFAT behavior is pinned by the notes
   file and a manual QA step, never a test.
9. ❌ **Every reversal can undo what this work makes.** A Locked or `deny delete` item Cmdr created must stay removable
   by the in-flight reversal (`write_operations/reversal.rs:155` `remove_file`, `:211` `remove_dir`), the ledger's
   `Drop` net (`ledger.rs:303-309`, `CopyTransaction`), the cross-FS move's staging cleanup (`cancellable.rs:32`,
   `remove_dir_all_in_background`), the in-flight sweep (`in_flight_sweep.rs:550`, `Surface::remove_file`, for rule 3's
   clone window), and the journal rollback (`rollback.rs:86`, `ReversalRunner`, `operation_log/DETAILS.md` §
   "Rollback"). Each clears the user flags and strips the ACL right before ITS OWN existing destructive act, after
   whatever check that path already makes (the reversal's recheck, the rollback's snapshot verify, the ledger's and
   sweep's own records), and adds no new check and no clearing where it removes nothing. A `deny delete` ACL blocks the
   owner's own `unlink` (verified), so clearing flags alone isn't enough.

## The end state

Every copy and move keeps every metadata class the destination can store, using the fastest mechanism that keeps all of
it, and tells the user once per operation which classes the destination couldn't hold. Per class:

- **mtime**: everywhere a protocol can store it (today's contract, unchanged).
- **Birth (creation) time**: everywhere a destination can store it: APFS, HFS+, exFAT, FAT (2 s / 10 ms resolution), SMB
  (`CreationTime`), the macOS SMB mount (smbfs forwards it), S3 (rclone's `btime`), Nextcloud WebDAV (if `X-OC-CTime`
  holds up). Impossible: SFTP v3, ADB, Linux local filesystems (birth is read-only via `statx`), MTP on Android (to
  verify).
- **atime**: kept where cheap and already done (local, SFTP's forced pair); never reported, never extended. Nobody reads
  it, and `noatime`/`relatime` make it meaningless on most destinations.
- **Mode**: local-to-local exactly (today); every routed source → local via the `landed_mode` fold (today); folders too
  (new); SFTP and ADB destinations sanitized (new); S3 as rclone's `mode` (new).
- **xattrs** (Finder tags, comments, FinderInfo with custom icons, resource forks, quarantine, app xattrs): local always
  (files today, folders new); SMB via named streams (blocked on vdavid/smb2#9); archives via `__MACOSX` AppleDouble
  (extract new; compress writes `__MACOSX` only when a source has xattrs worth keeping). Everywhere else impossible and
  reported. Skip `com.apple.provenance`.
- **Quarantine on extract**: every file and folder extracted from a quarantined archive carries the archive's
  `com.apple.quarantine`, like `ditto -x` and bsdtar (a security fix, shipped early as M2).
- **ACLs**: local-to-local, post-landing (moved there from the temp, which is a bug fix).
- **BSD flags**: `hidden`, `uchg` (Locked), `uappnd`, `nodump` local-to-local, post-landing (new on the chunked path);
  `hidden` ↔ SMB `FILE_ATTRIBUTE_HIDDEN` and Locked ↔ `FILE_ATTRIBUTE_READONLY` (new, needs a small smb2 change).
- **Symlinks**: stay links locally (today), plus the link's own dates (new, `FSOPT_NOFOLLOW`). Remote: SMB has none to
  give (the server resolves them); SFTP could (`SYMLINK`), and is out of scope here.
- **Folders**: everything a file gets, on folders the copy created, after their contents land.
- **Ownership**: never (rule 5).

## Fresh read of the current code

Paths are from the repo root; `T` = `apps/desktop/src-tauri/src/file_system/write_operations/transfer`.

### Entry points (all converge)

- Every user-facing start (F5/F6 dialogs, ⌘V paste of files, drag and drop, "Skip confirmation", queued starts, MCP, Ask
  Cmdr's suggested ops in `agent/suggested_ops/bridge/mod.rs:229-244`) reaches `routing.rs::start_volume_copy` /
  `start_volume_move` (`write_operations/routing.rs:206, 282`) or the plain `copy_files_start` / `move_files_start`
  (`write_operations/mod.rs:501, 569`). Both-local transfers go native; everything else goes to the volume engine. So
  the work lands in the engines, not per entry point.
- Routes that are NOT the two engines, and need their own line in a milestone: `archive_edit/copy_into.rs` (copy or move
  INTO an existing zip, via the mutator), `fresh_compress.rs`/`fresh_zip.rs` (Compress), `archive_edit/move_out.rs`
  (move out of a zip: extracts through the copy engine, then a `{ delete }` rewrite), `routing.rs::start_rename_by_move`
  (F2 and bulk rename on an object store: a same-volume move, `rename_work`), and S3's `copy_on_server`
  (`crates/cmdr-s3/src/volume/server_copy.rs:404-413`, `REPLACE` that restates every source `x-amz-meta-*`, so new keys
  ride it for free). The viewer's archive-entry temps (`file_viewer/materialize.rs`) are previews that never execute and
  stay out of scope.
- `paste_clipboard.rs` writes clipboard CONTENT as a new file: not a transfer, out of scope.

### Local to local (the native engine)

- **Strategy pick** (`T/copy_strategy.rs:165-187`): macOS same-APFS-volume → `AppleClone`, everything else (cross-volume
  APFS, HFS+, exFAT, FAT, the macOS SMB mount) → `Chunked`. Linux: `KernelCopyRange`, or `Chunked` on a network mount.
- **Every arm lands through `stage_and_land_file`** (`T/copy_strategy.rs:207-269` → `write_operations/overwrite.rs:67`):
  fill a `.cmdr-tmp-*` sibling, set the existing destination aside when replacing, one rename.
- **`AppleClone`** (`T/macos_copy.rs:279-386`, flags at :289):
  `copyfile(COPYFILE_ALL | COPYFILE_CLONE | NOFOLLOW_SRC | EXCL)` onto the TEMP. A clone keeps everything, birth
  included (verified). ❗ But the clone puts `uchg` on the temp and `COPYFILE_ACL` puts a `deny delete` ACL on it, and
  either one makes the landing rename fail (re-verified in review round 1 with the exact flags; notes § "The quirks").
  Expected today: copying a Locked file within one APFS volume fails and leaves a `uchg` temp the sweep can't `unlink`
  (`EPERM`, verified). ❗ Dropping `COPYFILE_STAT` doesn't help: `COPYFILE_DATA|XATTR|CLONE` still lands `uchg` on the
  temp (verified), because `clonefile` copies the flags. The only clone-keeping fix is clone, clear `uchg`/ACL on the
  temp, land, re-apply (verified to keep birth).
- **`Chunked`** (`T/chunked_copy.rs:65-101`): data, `sync_data`, then `copy_metadata` (`:280-294`): xattrs (`:297-336`,
  every name the `xattr` crate lists, failures per name are `debug!`), ACLs via `exacl` (`:340-358`, onto the TEMP, so
  the same rename hazard: a `deny delete` file fails to land TODAY on every chunked copy), `copy_timestamps`
  (`:367-382`, `filetime::set_file_times(atime, mtime)`, no birth), then `copy_permissions` (`:385-397`, full mode incl.
  setuid). The four are `?`-chained (rule 1). The temp is created with the default mode, so a `0600` file sits `0644`
  until the last step (rule 5). No BSD flags. Birth on APFS/HFS+ ends up == source mtime by the quirk; on exFAT/FAT it's
  "now" (verified). `transfer/DETAILS.md:1185` claims birthtime "matters only on same-volume copies"; that's wrong and
  gets corrected in M3.
- **Linux `KernelCopyRange`** (`T/linux_copy.rs:116-140`): mode and atime/mtime only, no xattrs, no ACLs. Linux can't
  set birth time at all.
- **Symlinks** (`T/macos_copy.rs:411-423`, used from `T/copy/single_item.rs`): `read_link` + `symlink`, a fresh link.
  Its own dates are "now" and its own xattrs are dropped.
- **Folders**: created bare, as FILE parents (`T/copy/single_item.rs:362`, `create_dir_all`), by the empty-folder pass
  (`T/copy/scanned_dirs.rs:85`), or eagerly for a self-duplicate (`T/copy/mod.rs:286`). After the file loop,
  `date_created_dirs_like_their_sources` (`T/copy/scanned_dirs.rs:109-141`) sets ONLY mtime, only on `created_dirs`,
  only on success, in scan order (not deepest-first). Everything else on a folder is lost: birth (== mtime by the
  quirk), mode (a `0700` folder lands `0755`: a privacy regression), tags, comments, FinderInfo (custom folder icons),
  ACLs, `hidden`, Locked.
- **Same-FS move** (`T/move_op/same_fs.rs`): `rename(2)`, keeps everything. ❗ A Locked item can't be renamed (`EPERM`),
  so a same-FS move of one fails today; that's not metadata loss, M0 pins it and it stays out of scope.
- **Cross-FS move** (`T/move_op/cross_fs.rs`): stages into a folder on the destination (`:117`), copies each file
  through `copy_single_item` (`:210`, so the `Chunked` gaps above, landing INTO the staging tree), dates the staged
  folders (`:310`), then Phase 3 renames each staged top-level item into place (`:452`), or for a merge renames the
  staged children one by one (`merge_move_directory`, `:356`), or resolves a conflict (`move_resolved_into_place`,
  `:424`); then the flush, then the source ledger delete. Every gap above becomes permanent loss here.
  - ❗ Phase 3 is a SECOND rename of everything staged, to a different parent. Any rename-blocker applied at the staging
    landing (rule 3) breaks Phase 3, and a failed Phase 3 hands the staging tree to `remove_dir_all_in_background`
    (`:463`), which can't remove a Locked or `deny delete` item either. So every post-landing class of a cross-FS move
    (ACL, flags, and a folder's final mode) is deferred to after Phase 3 and applied at the LANDED path (the `landings`
    map, `:323`).
  - ❗ Phase 3's `OverwriteOlder` compares the STAGED copy's mtime with the destination's (`T/linux_copy.rs:128-131`
    explains), so mtime on the staged copy is load-bearing for conflict logic.
  - A Locked SOURCE can't be deleted by Phase 4 (`EPERM`). M0 found the one-volume rig never gets there: its staged copy
    takes the clone arm, which carries `uchg` onto the temp, so the staging landing fails first (`EPERM`) and leaves
    that Locked temp in the staging tree, where no cleanup can remove it. On a real cross-volume move the chunked arm
    lands it unlocked and Phase 4's delete is what refuses; no cell pins that path yet.

### Cross-volume (the volume engine)

- **Contract**: `T/volume/DETAILS.md:806-905` § "Copies keep the source's date". The source reports
  `VolumeReadStream::modified_at` (`crates/cmdr-fs/src/volume/read_stream.rs:43`, required); the destination writes it
  inside `write_from_stream`. Explicitly out: birth time and permissions (`:824`).
- **Mode**: `T/volume/landed_mode.rs:46` folds the source's mode onto a LOCAL destination's temp before the rename,
  gated on `Volume::reports_posix_mode` (`crates/cmdr-fs/src/volume/mod.rs:956`; true for local, archive, git, ADB).
  `T/volume/DETAILS.md:740-804`. It's an ENGINE-side step (`apply_source_mode`, called from `T/volume/strategy.rs:557`
  and `T/volume/sequential_extract.rs:186`) that reaches the temp through `dest_volume.local_path()`, after
  `write_from_stream` returns and before the landing rename. With no listing in hand (a top-level file) it spends a
  `get_metadata` on the source per file (`landed_mode.rs:94-108`), a round trip on ADB. This is the one existing seam
  between "bytes written" and "landed" on the volume path; M2 uses it, M5a replaces it.
- **Folder dates**: `T/volume/folder_dates.rs:44-74`, through `Volume::set_modified`
  (`crates/cmdr-fs/src/volume/mod.rs:494`, defaults to `NotSupported`; the first `NotSupported` ends the pass).
  `folder_dates.rs:60` is its ONLY production caller (implemented by local, SMB, SFTP, `InMemoryVolume`), so widening it
  is a contained change.
- **Trait surface**: no birth, xattr, flag, ACL, or symlink-creation method. `FileEntry` carries `modified_at`,
  `created_at`, `added_at`, `opened_at`, `permissions` (0 = no mode), read-only Finder tags.
- **Local as a destination** (`apps/desktop/src-tauri/src/file_system/volume/backends/local_posix/streams.rs:249-263`):
  `File::set_modified` on the handle after the last byte. No birth, so SMB → local lands birth == mtime.
- **SMB** (`crates/cmdr-smb`, smb2 0.27.2):
  - Writes `LastWriteTime` only (`crates/cmdr-smb/src/volume/streams.rs:184-196`, and `:553-561` for the one-frame
    write); folders by path (`crates/cmdr-smb/src/volume/mutation.rs:70-86`).
  - smb2 already has `FileTimes::set_created` (`smb2/crates/smb2/src/client/times.rs:93`), and both `Tree::set_times`
    (by path, one CREATE+SET_INFO+CLOSE compound) and `set_handle_times` / `FileWriter::set_times` (one SET_INFO on the
    open handle) send all four times in the same `FILE_BASIC_INFORMATION`. So **writing CreationTime needs no smb2
    change and no extra frame**: Cmdr's `source_times` (`crates/cmdr-smb/src/volume/streams.rs:183`) adds one
    `.set_created(..)`. `FileAttributes` always goes as 0 (`times.rs:124-128`), so DOS attributes can't be set yet. New
    files are `FILE_ATTRIBUTE_NORMAL`.
  - Reads: `DirectoryEntry` and `FileInfo` carry `created` (`smb2/.../client/tree.rs:144-155, 224-235`), and Cmdr maps
    it to `FileEntry::created_at` (`crates/cmdr-smb/src/volume/mapping.rs:30-31`). ❗ The streaming download ALREADY has
    it for free: `FileDownload::info()` is the CREATE response's `FileInfo` (`smb2/.../client/stream.rs:270-282`), and
    Cmdr reads only `.modified` off it (`streams.rs:278`). So **reading CreationTime needs no smb2 change either**.
    Attributes are read only to decide `is_directory` (`tree.rs:3296`) and aren't on `FileInfo`/`DirectoryEntry`; that
    part does need smb2.
  - Write round trips today: the streaming writer stamps on its own handle (`stamp_writer`, `streams.rs:190`, one
    SET_INFO); the one-frame write is CREATE+WRITE+CLOSE and then a SEPARATE path `set_times` compound
    (`streams.rs:553-561`), so a small file already costs two round trips.
  - No named streams, no security descriptors in smb2's client. Symlinks: every entry is `is_symlink = false`; the
    server resolves links.
  - Server-side copy: `SmbVolume` doesn't override `copy_on_server`, so SMB always streams.
  - The temp rename (`FileRenameInformation`) never touches `FileBasicInformation`, so the stamped date survives.
- **SFTP**: `SETSTAT` with times only, atime = mtime (`crates/cmdr-sftp/src/volume/writes.rs:366`); folders via
  `set_modified`. Reads report file-type bits only, deliberately (`crates/cmdr-sftp/src/volume/mapping.rs:24-40`), even
  though the server's attrs carry permissions.
- **WebDAV**: `X-OC-Mtime` on the PUT (`crates/cmdr-webdav/src/volume/writes.rs:143`); reads `creationdate` into
  `created_at` (`crates/cmdr-webdav/src/volume/propfind.rs:143`). No folder dates.
- **S3**: `x-amz-meta-mtime` only (`crates/cmdr-s3/src/metadata.rs:14`), rclone's key and format; rclone's `btime`,
  `mode`, `atime` keys are neither written nor read. No folder dates (prefixes). Server-side copy and rename restate
  every `x-amz-meta-*` (`server_copy.rs:404-413`; `mutation.rs:297` uses `MetadataDirective::Copy`), so they already
  keep whatever M9 writes.
- **MTP**: `DateModified` in the `ObjectInfo` (`crates/cmdr-mtp/src/connection/file_ops.rs:293-296`); reads
  `DateCreated` into `created_at` (`directory_ops.rs:886`); mode 0 by design. No folder dates.
- **ADB**: `SEND path,mode` with a fixed `0o100660` (`crates/cmdr-adb/src/volume/streams.rs:32,196`), mtime in `DONE`
  (u32 seconds). No folder dates (no setstat in the sync protocol).

### Archives

- **Extract**: zip reads the `UT` (0x5455) mtime or the DOS time (`crates/cmdr-archive/src/read/zip_times.rs`), mode's
  low nine bits (`zip.rs:66`); tar mtime and mode (`tar.rs:126,143`), pax headers skipped (`tar.rs:108-112`, so
  `SCHILY.xattr.*` is dropped); 7z mode only. Symlinks extract as empty files, deliberately
  (`multiformat_test.rs:323-326`). No `__MACOSX` handling: AppleDouble entries extract as plain `._*` files. ❗ **No
  quarantine propagation**: extracting a quarantined download yields unquarantined files, which `ditto -x`, `tar -x`,
  and Archive Utility don't allow (verified for the first two). That's a Gatekeeper gap, not only lost metadata. Two
  routes reach it: a copy out (`T/volume/sequential_extract.rs`, the archive volume's read streams) and a move out
  (`archive_edit/move_out.rs`, which extracts through the same copy engine).
- **Compress** (zip only): DOS time + `UT` mtime and unix mode per entry
  (`crates/cmdr-archive/src/mutation/mutator.rs:440-480`, `fresh_zip.rs:432-438`); symlinks and special files skipped
  (`fresh_plan.rs`); no xattrs, no `__MACOSX`. Two writers: a fresh zip (`fresh_zip.rs`) and adding to an existing one
  (`archive_edit/copy_into.rs` → the mutator).
- **Editing a zip in place** keeps the ARCHIVE file's own metadata with `copyfile(STAT|ACL|XATTR)`
  (`crates/cmdr-archive/src/mutation/DETAILS.md:52-56`). Its claim that `COPYFILE_STAT` carries birth time is wrong for
  a non-clone copy (verified: it doesn't), so a zip edit resets the archive's birth to its mtime. It also copies a
  `deny delete` ACL or `uchg` onto the rewritten temp before its rename: the same rule-3 hazard (a Locked zip can't be
  edited anyway, but an ACL'd one can). Fixed in M3 with the shared helper.

## Gap matrix (today → target)

"keep" = preserved today; "LOST" = the destination can hold it and we drop it; "n/a" = the destination can't hold it;
"FAIL" = the copy itself fails over metadata (a rule-3 bug).

- **Local, same APFS volume (clone)**: files keep everything, but Locked or `deny delete` files FAIL to land (bug).
  Symlink dates LOST. Folders: birth, mode, xattrs, ACL, flags LOST.
- **Local, other APFS / HFS+ volume (chunked)**: `deny delete` files FAIL to land (ACL on the temp); file birth LOST
  (shows mtime), flags LOST, xattrs/mode/mtime/atime keep (mode only if the time-set succeeded, rule 1). Folders as
  above. Symlink dates LOST.
- **Local, exFAT / FAT**: file birth LOST (shows now), `hidden`/Locked LOST (FAT attributes can hold them), xattrs keep
  as `._` sidecars, resource forks n/a (the FSKit drivers refuse them, notes § "Results"), ACLs/mode n/a. A temp's
  sidecar, birth, and mtime survive the landing rename on exFAT (verified in review round 1).
- **Local, macOS SMB mount (chunked through smbfs)**: as other-volume; birth should be settable through smbfs (to verify
  in M3's QA, against the Docker share mounted with `mount_smbfs`), xattrs depend on the server's `vfs_fruit` (the
  fixture has none, so the QNAP-shaped answer stays unverified until M7's fixture).
- **Same-FS move**: keeps everything; a Locked item FAILS (`rename` `EPERM`, out of scope). **Cross-FS local move**: the
  chunked row, made permanent; a Locked source stays behind (Phase 4 can't delete it).
- **Local → SMB (direct)**: mtime keep; birth LOST (`CreationTime` = now; the fix needs no smb2 change, M5a);
  hidden/Locked LOST (needs smb2 attributes); xattrs LOST (needs smb2#9 + a server with streams); mode/ACL n/a. Folders:
  mtime keep, the rest as files.
- **SMB → local**: mtime keep; birth LOST (the CREATE response and listing already carry it); hidden/readonly LOST;
  xattrs LOST (smb2#9).
- **SMB → SMB copy** (any share: `SmbVolume` has no server-side copy, so even a same-share copy streams): mtime only;
  birth LOST. A move within one share renames and keeps all.
- **Local ↔ SFTP**: mtime keep; birth n/a; mode LOST both ways (attrs carry `permissions`); xattrs n/a.
- **Local ↔ WebDAV**: mtime keep; birth LOST toward Nextcloud only if `X-OC-CTime` works (to verify), else n/a; folder
  dates n/a; the rest n/a.
- **Local ↔ S3**: mtime keep; birth and mode LOST (rclone keys exist); folders n/a. S3 → S3 server copy keeps every
  `x-amz-meta-*`.
- **Local ↔ MTP**: mtime keep; birth to verify (`DateCreated` is a field Android likely ignores), folder dates n/a.
- **Local ↔ ADB**: mtime keep; mode sends a fixed `0660` (low value: `/sdcard` ignores modes); birth and folder dates
  n/a.
- **Archive → local** (copy or move out): mtime and mode keep; xattrs from `__MACOSX` LOST; quarantine LOST (security);
  tar pax xattrs LOST; folder dates keep.
- **Local → zip** (fresh or into an existing one): mtime and mode keep; xattrs LOST; birth n/a in zip.

## Best practice (what the tools keep)

- **`ditto`** (verified, macOS 27): mtime, atime, mode, flags, ACLs, xattrs, resource forks (AppleDouble on FAT). Birth
  only where it clones (same APFS volume, files only). Folder birth never.
- **`cp -p`** (verified): like `ditto` minus the clone, so birth never.
- **`rsync -aNHAX --crtimes`** (verified, 3.5.1): birth on files AND folders, xattrs, ACLs, hard links, symlinks with
  their own times. No BSD flags in this build. Its temp+rename trips on a `deny delete` ACL exactly like Cmdr would.
- **Finder**: reported to keep "Date Created" on Mac-format destinations, plus tags, comments, and Locked; not verified
  here (automating Finder needs a TCC prompt). Open question 1.
- **Total Commander / ForkLift**: keep mtime; creation-time and attribute behavior not verified (TC has explicit options
  for folder dates and attributes on Windows). Neither is the bar; `rsync --crtimes` plus flags is.
- **The bar we pick**: `rsync -aNHAX --crtimes` + BSD flags + folders + Finder's quarantine discipline on extract.

## Telling the user

**Recommendation: one calm, per-operation note when something the SOURCE had couldn't be kept. No setting, no prompt, no
per-file noise.**

- **Why not silent**: radical transparency (`docs/design-principles.md`); a person who moves photos to a NAS deserves to
  know their creation dates didn't make it before the originals are gone.
- **Why not a setting**: there's no reason to want less metadata kept, and nothing to choose when a destination can't
  hold it. A setting would only let people turn off an honest sentence.
- **Why not a prompt**: it would interrupt every transfer to a phone or bucket with a question that has one answer.
- **What counts**: only classes a person can see (creation dates, Finder tags and comments, permissions, Locked and
  Hidden, folder dates), and only when at least one source item actually had a value worth keeping (a tag, a mode a
  plain file wouldn't get, a birth time earlier than its mtime). atime, ACLs on a non-ACL destination, `provenance`, and
  quarantine never appear.
- **Where**: the completed operation's row in the operation queue and its toast's secondary line, plus one `info!` log
  line naming the classes and counts. i18n through the translator agent, never the coding agent.
- **Wording: PLACEHOLDER, David rewrites it before it ships.** "Copied 214 files. This server can't store creation dates
  or Finder tags, so those stayed behind." The implementer ships this exact placeholder with a `@key` description that
  says it's a placeholder, and the milestone's report reminds David.
- **How it's computed**: the destination's `MetadataCaps` (M5b) says what it can hold; the engine counts, per class,
  source items that had a value and weren't kept (caps say no, or a best-effort write was refused, read off the
  `MetadataReport`s). The counts ride the `WriteCompleteEvent`. The state is a fixed array of per-class counters on
  `WriteOperationState`, never a per-file list, so a 100,000-file copy costs a few bytes.
- **"Had a value" must be free to know**: from the stat and listing in hand (birth vs mtime, mode, flags), and for
  xattrs on a LOCAL source one name-only `flistxattr` on the fd the copy already holds (so `provenance`/quarantine-only
  files don't count). A remote source that can't say for free reports "unknown", which is never counted (rule 6).
- ❗ **Asked only for classes the destination can't store.** Where the caps say a class is kept, there's nothing to
  report and nothing to ask, so a local → local copy (and the clone arm's one-`copyfile` fast path) never pays the
  `flistxattr` (invariant 10).

## Design sketch (fixed in M1, M3, and M5a, sketched here so nothing paints into a corner)

One host module, one trait method, one stream accessor (M8 adds one default-empty folder read, see M8). Every piece
below has exactly one home, and each is born in the milestone that first needs it:

- **M1**: `cmdr_fs::metadata::{MetadataReport, LandedMetadata, apply_landed}`.
- **M3**: `CarriedMetadata` and `apply_staged`.
- **M5a**: `MetadataClasses`, `VolumeReadStream::metadata`, and `Volume::apply_metadata`.
- **M5b**: `MetadataCaps`.

The pieces:

- **The host helper lives in `cmdr-fs`, from M1** (`crates/cmdr-fs/src/metadata/`, beside `staging.rs`: a host
  primitive, no `tauri`). ❗ Not under `apps/desktop/.../transfer/`: M3 routes the zip editor
  (`crates/cmdr-archive/src/mutation/mutator.rs:565-591`, its own `copyfile`) through it, and a crate can't reach the
  app. `cmdr-fs` already depends on `xattr` (every OS) and `libc` (macOS ONLY, a block `cfg-gate` watches: the Linux
  half uses std and `exacl`, never `libc`). `exacl` moves there from the app's `Cargo.toml` (same `0.13`, already
  `cargo deny`-clean), and leaves the app's once its last use there goes (`cargo-machete`). Its callers: the clone arm,
  the chunked arm, the native folder pass, the cross-FS move's deferred pass, the local volume's `write_from_stream` and
  `apply_metadata`, and the zip editor. One implementation of the rule-2 and rule-3 ordering.
- **Two halves, both total**: `apply_staged(target, &CarriedMetadata) -> MetadataReport` (xattrs, a file's mode, then
  one dates call; everything rule 3 allows on a temp) and `apply_landed(path, &LandedMetadata) -> MetadataReport` (a
  folder's final mode, then ACL, then flags: `uchg` blocks a later `chmod` too). A folder's `apply_staged` carries no
  mode; its mode is in `apply_landed`, after its children land (M4). On a file the copy holds open, the staged half
  works on the fd (`fsetxattr`, `fchmod`, `File::set_times`): no path lookups, and an fd write on a `0444` file isn't
  refused the way a path write is. Paths only where there's no fd (the clone's temp, a folder, a symlink with
  `NOFOLLOW`).
- **`MetadataReport`** (M1, in the helper): per class, `Kept`, `Refused`, `NotApplicable`, or `Absent` (the source had
  nothing). Every apply function returns one and has no `Result`, which is what makes rule 1 a type fact. M5b sums it
  into the loss counts; it never moves crates.
- **`LandedMetadata`** (M1): the post-landing set: the ACL (an `exacl` value, local-to-local only), the user flags, and
  a folder's final mode. Small, `Clone`, and cheap to hold in a sparse map.
- **`CarriedMetadata`** (in `cmdr-fs`, M3): `created`, `mode`, `flags` (a small typed set: `Hidden`, `Locked`,
  `AppendOnly`, `NoDump`), and `xattrs`. No `modified` (`VolumeReadStream::modified_at` stays the ONE source of mtime,
  so its 20-odd implementers don't migrate and two getters can't disagree) and no `accessed` (the end state never
  extends atime, so nothing would read it). ACLs aren't in it: they only travel local-to-local, natively.
- **The stream accessor is lazy**: `VolumeReadStream::metadata(&self, wanted: MetadataClasses) -> CarriedMetadata`,
  default empty. The DESTINATION calls it inside `write_from_stream` with the classes it can store, so a local source
  reads xattrs (`fgetxattr` on the fd it already holds) only when the destination will keep them, and a local → SFTP
  copy pays nothing for tags. Remote sources answer from what the open returned (SMB's `FileDownload::info()`), never a
  new request. xattrs above a size cap (resource forks can be large) are dropped and counted; the cost is bounded by the
  transfer's concurrency window, since only in-flight files hold one.
- **Mode rides the stream too**, which retires `apply_source_mode`'s engine-side `local_path()` reach-in and its
  per-top-level-file `get_metadata` round trip (`landed_mode.rs:94-108`). The `landed_mode` FOLD stays (as a pure
  function the local destination calls); the seam moves into the local volume's `write_from_stream`, after the xattrs
  and before the dates (rule 2). Every stream with a mode reports it from what its open already holds: local (the open's
  `metadata`, `local_posix/streams.rs:113`), archive (the index entry, in both `open_read_stream` and the sequential
  `MemberStream`), git (the tree entry), and ADB (the `stat` its open already sends,
  `crates/cmdr-adb/src/volume/streams.rs:61`, through `ChannelReadStream`). ❌ Missing one of the four turns today's
  probe into a lost mode.
- **Extract quarantine stays engine-held.** M2 reads the archive's `com.apple.quarantine` once per operation; from M5a a
  small engine decorator stream (beside `checkpoint_stream.rs`) merges it into `metadata()`'s xattrs, and
  `FolderMetadata` adds it to each created folder's set. `cmdr-archive` never reads its parent's xattrs.
- **One trait method for "apply by path"**: `Volume::set_modified` (one production caller, `folder_dates.rs:60`) becomes
  `Volume::apply_metadata(path, &CarriedMetadata, modified: Option<SystemTime>) -> MetadataReport`, default
  `NotApplicable` for every class. It serves BOTH a folder's full set after its subtree lands AND a file's
  rename-blocking subset right after its landing rename (Locked, SMB `READONLY`). Two near-identical methods
  (`finish_landed` + `apply_folder_metadata`) would be the same body twice on every backend. `FolderDates` becomes
  `FolderMetadata`, still stamped after the subtree lands, deepest first, and still stops asking once a volume answers
  `NotApplicable` for everything.
- **The engine snapshots the post-landing subset** (flags only, a few bytes) off the source stream before handing the
  stream to `write_from_stream`, which consumes it. Only when that subset is non-empty does it spend an `apply_metadata`
  after the rename: a plain file costs nothing.
- **`MetadataCaps`** is a `Volume` predicate (`fn metadata_caps(&self) -> MetadataCaps`), per `cmdr-fs`'s rule that
  `capabilities()` is a pure fold of predicates, never overridden. `reports_posix_mode` folds into it. It says which
  classes the volume can STORE, which it can REPORT for free, and whether folders take them. Born in M5b; M5a's
  destinations decide from their own type what to ask for.
- **Deferral, not re-reading, and sparse.** The native cross-FS move remembers the post-landing set (from the source's
  `lstat` and `exacl` read at copy time) ONLY for items that have one, keyed by landed path, so a 100,000-file move of
  ordinary files holds an empty map, and Phase 3 applies it without re-reading the source.

## Resource cost (the per-file budget)

The common case (a plain file, no flags, no ACL, no xattrs beyond `provenance`) must cost what it costs today:

- **Clone arm**: today one `copyfile`, and no ACL read at all (only the chunked arm calls `getfacl`). So M1 asks nothing
  up front: the landing rename is the probe. Only when it fails `EPERM`/`EACCES` does the arm read the temp's flags and
  ACL, clear the blockers, retry the rename once, and re-apply them landed. A plain file stays one `copyfile` + one
  `rename`.
- **Chunked arm**: today listxattr + get/set per name + ACL read/write + `utimes` + `chmod`. After: the same xattr
  calls, `fsetattrlist` REPLACES `utimes` (one call either way), `fchmod` replaces `chmod`, the ACL write and `chflags`
  happen only when the source has one. Net zero for a plain file.
- **Folders**: one `setattrlist` per created folder replaces today's `utimes`; xattr calls only for folders that have
  some; mode and flags as above. A 10,000-folder tree adds ~10,000 `listxattr`s on the source, local syscalls.
- **SMB**: CreationTime and Hidden ride the SET_INFO that already carries LastWriteTime: zero new frames. `READONLY`
  costs one path compound per Locked file, only for those. Folders: same one compound per folder as today.
- **Optional win (verify first, M5a)**: fold the one-frame write's separate date stamp into the compound itself
  (CREATE+WRITE+SET_INFO+CLOSE). An explicit time set on a handle should stop that handle's CLOSE from restamping it
  (MS-FSA "sticky" times; Samba honors it), which would save one round trip per small file. Measure on the Docker
  fixture; the QNAP only with David's go-ahead, since the measurement writes. If the CLOSE wins, keep today's two
  frames.
- **Memory**: per-class counters, a sparse deferral map, and xattrs only for in-flight files.

## Platforms

Every macOS-only call is `#[cfg(target_os = "macos")]`, and the helper exposes the same two halves on Linux with those
classes answering `NotApplicable`. The `desktop-rust-clippy-linux` and `desktop-rust-tests-linux` lanes must pass on
every milestone that touches the helper or the native engine.

- **macOS-only**: `copyfile`, `setattrlist` / `fsetattrlist`, `chflags`, `FileTimesExt::set_created` (Apple and Windows
  only in std), `st_birthtime`, `FSOPT_NOFOLLOW`, `com.apple.*` names.
- **Linux, as a source**: birth reads through `Metadata::created()` (`statx` btime), so a Linux → SMB copy keeps
  creation dates; `user.*` xattrs; POSIX ACLs via `exacl`; mode.
- **Linux, as a destination**: birth can't be set (report n/a). Dates via `File::set_times` (`futimens`), mtime and
  atime. xattrs in `user.*` only (`trusted.*`, `security.*` need privileges; `system.posix_acl_*` is the ACL, done by
  `exacl`). Linux refuses a name without a namespace prefix, so a macOS `com.apple.*` xattr reaching a Linux destination
  is n/a and reported; ❌ don't invent a `user.com.apple.*` mapping. POSIX ACLs carry no `deny` entries, so they don't
  block a rename, but they still go through `apply_landed` for one shape on both OSes.
- **Linux flags are n/a**: `FS_IOC_SETFLAGS` immutable and append-only need `CAP_LINUX_IMMUTABLE`, which a desktop app
  doesn't have, and `nodump` alone isn't worth a code path. Hidden on Linux is a dot-name, not metadata.

## Milestones

Order runs by user value: M0 pins reality; M1 (the Locked landing bug) and M2 (extract quarantine) ship early, as David
decided; then the ask itself, creation and modification dates on files AND folders: local files (M3), local folders
(M4), across volumes with SMB first (M5a, which needs no smb2 release). The loss note (M5b) and the rest follow.

**Dependencies and parallel lanes** (each milestone lists the files it owns; two that share a file never run at once):

- M0 → everything.
- M1 ∥ M2: disjoint files (M1: native engine, `overwrite.rs`, the rule-9 reversal files, `in_flight_sweep.rs`,
  `cmdr-fs/src/metadata/`; M2: `T/volume/strategy.rs`, `sequential_extract.rs`, `landed_mode.rs`, `merge.rs`). M2 writes
  its xattr with a plain `xattr::set`, the one sanctioned exception to invariant 11, which M5a removes.
- M3 after M1 (`chunked_copy.rs`, `macos_copy.rs`, `linux_copy.rs`, the helper's `apply_staged`).
- M4 after M3 (the helper's folder shape, `move_op/cross_fs.rs`).
- M5a after M2 AND M3: the local destination calls `apply_staged` with birth on the fd, which M3 builds. M4 ∥ M5a: M4
  owns the native engine, M5a the volume engine, `cmdr-fs/src/volume/`, `cmdr-smb`; both only CALL the helper. If either
  must change the helper's signature, it stops and says so.
- M5b after M5a. Then M6, M8, and each M9 backend in parallel (separate crates; M8 owns `cmdr-archive`,
  `sequential_extract.rs`, `FolderMetadata`, `archive_edit/`, and the one default-empty `Volume::folder_metadata` it
  adds to `cmdr-fs`; M6 owns `cmdr-smb`; each M9 backend its own crate). M7 is blocked on smb2#9 and shares `cmdr-smb`
  with M6, so it goes after M6.
- M10 last.

### M0: Characterization (pin today, red where it's a bug). Size: M (1 session, ~600 test lines)

- **Scope**: tests only, plus one fixture builder. No production line changes.
- **The fixture builder**: `crates/cmdr-fs/src/testing/metadata_fixture.rs` (under the `testing` feature, beside
  `disk_images`, so M3's `cmdr-archive` tests reuse it). It builds the notes file's fixture (§ "The fixture") in a
  `TestDir`: birth through `setattrlist` (`libc`, macOS), flags through `libc::chflags`, xattrs through `xattr`, and the
  ACL by running `chmod +a "everyone deny delete"` (the way `error_classification.rs:213` runs `chflags`; `exacl` isn't
  in `cmdr-fs` until M1). Its guard's `Drop` runs `chflags -R nouchg,nouappnd` and `chmod -R -N` over the whole dir
  BEFORE `TestDir` drops, so a `uchg` `.cmdr-tmp-*` the copy left behind doesn't fail the cleanup.
- **The cells** (`#[cfg(target_os = "macos")]` unless noted), each through a seam that already exists:
  - **Clone arm**: `copy_file_using(LocalCopyStrategy::AppleClone, ..)` (`T/copy_strategy.rs:207`) within one `TestDir`.
    Assert `select_local_copy_strategy` answers `AppleClone` for the pair first, or the pin passes for the wrong reason.
  - **Chunked arm**: `copy_file_using(LocalCopyStrategy::Chunked, ..)` within one `TestDir`, the seam
    `volume/copy_crashsafe_tests.rs:616` already uses. Same landing, no disk image, so it runs in `rust-tests`.
  - **Folders**: `copy_files_with_progress_inner` on a fixture folder (the folder pass is strategy-independent).
  - **Cross-FS move**: `move_op/test_support.rs::run_cross_fs_move` (`move_with_staging` on one volume; its copies take
    the clone arm).
  - **Same-FS move** of a Locked file: `run_same_fs_move`.
  - **Extract**: a quarantined zip (made in the test with `zip`, quarantine set with `xattr`) copied out through the
    volume engine, as `volume/copy_extract_out_tests.rs` does.
  - **One real cross-volume cell** (APFS → HFS+ through `copy_files_with_progress_inner`) in `T/real_image.rs`,
    `#[ignore]`d. It's already a `disk-images` lane atom; ❌ a NEW module would also need a `diskImageLaneTestAtoms`
    entry and a `disk-image` override in `.config/nextest.toml`.
  - **SMB birth, both ways**: a scenario in `backend_suites/network_dates_test_support.rs`, wired in
    `smb_transfer_semantics_test.rs` (Docker fixture). The other backends join in M9.
  - **Linux** (`#[cfg(target_os = "linux")]`): `user.*` xattrs and mode on the `Chunked` and `KernelCopyRange` arms.
- **Cell file**: `T/metadata_matrix_tests.rs`, registered in `T/mod.rs` beside the other `*_tests` modules; the network,
  extract, and image cells live in the files named above. One test per cell, named for the behavior
  (`a_locked_file_cloned_within_one_volume_fails_to_land_today`), with a `// FLIPS IN M<n>` comment.
- **Pinned bugs** (assert today's outcome; M1 flips the first four):
  - a Locked file, clone: the copy fails and a `uchg` temp stays behind;
  - a `deny delete` file, clone AND chunked: fails to land;
  - a cross-FS move of a folder holding a `deny delete` file: fails at the staging landing;
  - a cross-FS move of a Locked source: fails at the staging landing today, before Phase 4 (M0 finding, § "Fresh read");
    M1 pins what Phase 4 then does with the Locked source;
  - a same-FS move of a Locked file: `EPERM` (out of scope by David's decision; filed as a follow-up in M10);
  - an extract of a quarantined zip: no quarantine (flips in M2).
- **Pinned losses** (flip in M3, M4, M5a): file birth on the chunked arm, flags on the chunked arm, symlink dates, every
  folder class, SMB birth.
- **Red first, for a characterization test**: write each cell asserting the TARGET behavior, run it, and see it fail for
  the documented reason (the errno, the missing value). Then invert the assertion to today's outcome. That's the proof
  the cell bites, and it's what the later milestone flips back.
- **Landmines**: don't assert FAT/exFAT (rule 8). Read birth with `std::os::macos::fs::MetadataExt::st_birthtime` or
  `Metadata::created()`. `com.apple.provenance` lands on every written file: exclude it from xattr comparisons.
- **Lanes**: `pnpm check rust-tests clippy cfg-gate`, `pnpm check rust-integration-tests` (the SMB cells),
  `pnpm check disk-images` (the image cell), and `pnpm check clippy-linux rust-tests-linux` (the Linux cells).
- **DONE**: every cell above exists, is green on today's code, and was seen red first; the commit body lists every
  pinned cell with the milestone that flips it.

### M1: Rename-blocking metadata lands after the rename (the Locked bug). Size: M (1 session)

- **Scope**:
  - The helper's first cut: `crates/cmdr-fs/src/metadata/` with `MetadataReport`, `LandedMetadata`, and `apply_landed`
    (§ "Design sketch"). `exacl = "0.13"` moves to `cmdr-fs`'s `Cargo.toml`. No `apply_staged` yet.
  - The clone arm (`T/macos_copy.rs:279-386`, `T/copy_strategy.rs:219-231`) and the landing
    (`write_operations/overwrite.rs:67`, `stage_and_land_file`).
  - The chunked arm's ACL step (`T/chunked_copy.rs:340-358`, `copy_acls`).
  - The cross-FS move's deferred pass (`T/move_op/cross_fs.rs`: collect during the copy at `:210`, apply after Phase 3
    through `landings` at `:323`, before the flush).
  - Rule 9's five paths, at the lines rule 9 names, and `operation_log/types.rs:224` (`SkipReason`).
- **Intentions**:
  - `apply_landed` applies ACL, then flags, each only when the set has one, each independent (rule 1).
  - Clone: keep `COPYFILE_ALL|CLONE` (the one call that keeps birth, and instant). The landing rename is the probe (§
    "Resource cost"): on `EPERM`/`EACCES`, read the TEMP's flags (`lstat`) and ACL (`exacl::getfacl`), clear
    `UF_IMMUTABLE|UF_APPEND` and strip the ACL on the temp, retry the rename once, then `apply_landed` what was cleared.
    A second refusal (the parent itself refuses) surfaces the ORIGINAL error, and the temp, now cleared, is removable.
    Verified to keep birth and land `uchg`.
  - Chunked: `copy_acls` reads the source ACL where it does today, and the set goes to `apply_landed` after the landing
    instead of `setfacl` on the temp. (Flags on the chunked path are new behavior and belong to M3.)
  - `stage_and_land_file` gains the post-landing step: after the rename (and after the aside swap on an Overwrite), on
    the FINAL path. In a cross-FS move's staging tree it doesn't apply; it hands the set back to the caller.
  - Cross-FS move: collect each item's `LandedMetadata` ONLY when it has one (a sparse
    `HashMap<PathBuf, LandedMetadata>` keyed by staged path), map it through `landings` after Phase 3, apply at the
    final path. A Phase 3 failure then leaves a staging tree `remove_dir_all_in_background` can remove.
  - Rule 9, all five paths: on the item about to be removed, clear user flags and strip the ACL, then remove. The
    journal rollback does it in `ReversalRunner` (it acts through the `Volume` trait; clear only when
    `volume.local_path()` is `Some`). The sweep does it only after its first `remove_file` fails `EPERM`/`EACCES`.
  - Linux: `apply_landed` does the POSIX ACL only; flags answer `NotApplicable` (§ "Platforms").
- **Landmines**:
  - A post-landing step on an Overwrite runs on the NEW file; the aside is the old one. Don't confuse them.
  - A crash between landing and post-landing leaves a file without its ACL/flags. Accept it and say so in the
    `transfer/DETAILS.md` section M1 adds; no ledger entry.
  - ❌ Don't make the user's own F8 clear Locked: the journal rollback deletes through `LocalPosixVolume`, which is also
    F8's path, so the clearing lives in `ReversalRunner`, never in the volume.
  - `SkipReason`: an item that fails the rollback's verification already gets `UnverifiablePrecondition` or `Drift`;
    keep that. Add ONE variant, `Locked => "locked"`, for a VERIFIED item whose unlock is refused (`schg`, which
    non-root can't clear). Its token is stored in the journal: extend the round-trip test in `types.rs`.
  - `chflags` on the clone's temp is a path call: a `0444` clone can't be opened for writing.
- **Tests** (red first: each new cell fails on today's code for the documented errno, before the fix):
  - M0's four landing cells flip in `T/metadata_matrix_tests.rs`: a Locked file, clone, lands Locked with birth kept; a
    `deny delete` file lands with its ACL, clone AND chunked; a cross-FS move of a folder holding both lands both.
  - A forced Phase 3 failure (a file squatting the final name, the way `move_op/move_op_tests.rs` forces conflicts)
    leaves no `uchg` or ACL'd item in the staging tree.
  - Rule 9, one cell per path, each with a Locked copy AND a `deny delete` copy: the in-flight reversal
    (`reversal_tests.rs`), the ledger `Drop` (`ledger_tests.rs`), the cross-FS move's staging cleanup
    (`move_op/move_reversal_tests.rs`), the journal rollback (`operation_log/rollback/` tests), and the sweep
    (`in_flight_sweep_tests.rs`, a `uchg` temp recorded as ours).
  - A plain file through the clone arm still makes no ACL or flag call (assert on a counting seam in the helper, or
    review it in the report if no seam is cheap).
- **Lanes**: `pnpm check rust-tests clippy cfg-gate cargo-deny cargo-machete module-cycles`, then
  `pnpm check clippy-linux rust-tests-linux` (the helper builds on Linux).
- **DONE**: no copy or move fails over metadata; every rule-9 path has a test; the commit names the flipped cells.

### M2: Extract keeps Gatekeeper's quarantine. Size: S (1 session)

- **Scope**: the volume engine's extract paths (`T/volume/strategy.rs`, `sequential_extract.rs`, `landed_mode.rs`,
  `merge.rs`), which also cover a move out of a zip (`archive_edit/move_out.rs` extracts through the copy engine;
  confirm with a test, don't assume). A zip is random-access, so it extracts through `stream_pipe_file`; a compressed
  tar is sequential (`ArchiveFormat::is_sequential`). No `cmdr-archive` change.
- **Intentions**: read the ARCHIVE file's `com.apple.quarantine` once per extract (only when the archive is local) and
  write it on every extracted file AND folder before its landing rename (on the temp, rule 2's xattr slot), the way
  `ditto -x` and bsdtar do (verified). A copy of the archive itself already keeps it (`copy_xattrs`).
- **The seam (no dependency on M5a)**: the existing engine step between "bytes written" and "landed",
  `landed_mode::apply_source_mode` (called at `T/volume/sequential_extract.rs:186`, and `strategy.rs:557` for a
  non-sequential archive copy), which already reaches the local temp via `dest_volume.local_path()`. Put the quarantine
  write right BEFORE the `chmod` there (xattrs before mode, rule 2), on the same path, with a plain `xattr::set`. The
  value is read once per operation and held in the engine's operation context, where M5a keeps it.
- **Folders**: right after `merge.rs`'s `create_directory` succeeds (`:477`, `:497`), so only folders this operation
  CREATED get it (rule 4). ❌ Not in `folder_dates.rs`: that pass skips a folder with no source date (a zip's implicit
  folders) and every folder of a cancelled or failed subtree, which would leave an extracted `.app` bundle unquarantined
  around quarantined files. An xattr on a fresh, empty folder blocks nothing and bumps no date.
- **Landmines**: a remote archive (SMB, MTP) has no xattr to read: nothing to propagate, and that's honest (Finder
  doesn't either). A path xattr write on a `0444` temp is `EACCES`, which is exactly why it goes before the `chmod`.
  Don't touch `__MACOSX` here (M8). macOS-only (`cfg`); Linux has no quarantine.
- **Test plan** (in `volume/copy_extract_out_tests.rs`; red first, the M0 cell flips): a quarantined zip extract (files
  and folders, implicit folders included, carry the value byte-for-byte), a cancelled extract (the folders it made are
  quarantined), a quarantined `.tar.gz` (the sequential path), a move-out cell, an unquarantined zip (nothing added).
- **Lanes**: `pnpm check rust-tests clippy cfg-gate`, then `clippy-linux`.
- **DONE**: extracting a quarantined Finder-made zip yields quarantined files and folders.

### M3: Local files keep everything (birth, flags, symlinks). Size: M (1–2 sessions)

- **Scope**: `chunked_copy.rs`, `macos_copy.rs` (symlinks), `copy_strategy.rs`, the host helper
  (`crates/cmdr-fs/src/metadata/`: `CarriedMetadata` and the `apply_staged` half are born here), Linux `user.*` xattrs
  and POSIX ACLs on `KernelCopyRange` (`linux_copy.rs`), the zip-edit archive-file metadata
  (`crates/cmdr-archive/src/mutation/mutator.rs:565-591`, replacing its private `copyfile` FFI) through the same helper.
- **Intentions**:
  - Chunked: create the temp `0600` (rule 5); data; then, on the open fd, xattrs, `fchmod`, then ONE `File::set_times`
    with `set_created` (std's `FileTimesExt`, Apple-only; std 1.97.1 issues it as one `fsetattrlist`,
    `sys/fs/unix.rs:1792`) + modified + accessed, before `sync_data`. Each step independent (rule 1). After landing
    (M1's hook): ACL, then user flags (new: `hidden`, `uchg`, `uappnd`, `nodump`), each only when the source has one.
  - Symlinks: the link's own birth/mtime via `setattrlist(FSOPT_NOFOLLOW)` (verified), its xattrs with `XATTR_NOFOLLOW`.
    macOS only: on Linux a link's dates stay "now" (setting them needs `utimensat(AT_SYMLINK_NOFOLLOW)`, and `cmdr-fs`
    has no `libc` on Linux), reported as n/a.
  - Skip `com.apple.provenance` in the xattr copy (check `copyfile`'s own skip behavior, match it).
  - Linux: copy xattrs in the `user.*` namespace only (`system.posix_acl_*` is the ACL, done by `exacl`; `trusted.*` and
    `security.*` need privileges), no birth.
  - The zip edit: the archive's own metadata through the helper's two halves (birth kept; ACL and flags after the
    rewrite's rename).
  - Correct `transfer/DETAILS.md:1185` and `cmdr-archive/src/mutation/DETAILS.md` § "Metadata preservation".
- **Landmines**:
  - Rule 2: one call. `filetime` can't set birth; `File::set_times` replaces it on both OSes (Linux: no `set_created`,
    `cfg`-gated, § "Platforms").
  - Per-file cost: no new syscall for a plain file (§ "Resource cost"). No test counts syscalls cheaply, so the
    milestone's report lists every new call on the chunked and clone arms with its condition.
  - The cross-FS move's `OverwriteOlder` reads the staged copy's mtime: keep mtime on the temp.
  - xattrs before mode: a path xattr write on a `0444` temp is `EACCES` (verified).
- **Test plan**: M0's matrix flips green for files; new cells: Locked file cross-volume lands Locked; birth < mtime and
  birth > mtime both survive (verified possible); a `0600` source's temp is never group/other-readable; a refused
  time-set still applies the mode.
- **QA beyond the tests**:
  - The agent: a macOS SMB mount. `mount_smbfs` mounts the Docker guest share (the SMB fixture's own mount tests do,
    `test/smb-servers/README.md`); copy a fixture file onto it through Cmdr's native engine and read birth and tags
    back. Record the result with an evidence anchor in `transfer/DETAILS.md`, and fix the gap matrix's "to verify".
  - David, by hand: an exFAT USB stick (rule 8 bars an image), birth and tags, against the notes file. The agent's
    report asks for it.
- **DONE**: no class lost on local files except ownership; docs corrected.

### M4: Local folders keep everything. Size: M (1 session)

- **Scope**: `copy/scanned_dirs.rs` (`date_created_dirs_like_their_sources` → a full folder-metadata pass), folder
  creation sites (`copy/single_item.rs:362`, `scanned_dirs.rs:85`, `copy/mod.rs:286`), cross-FS move's staged folders
  and M1's deferred pass. Native engine only: the volume engine's folders (`folder_dates.rs`) are M5a's. The folder path
  of the helper (`apply_staged` by path, `apply_landed` with the final mode) is called, and if M3 left it incomplete,
  finished here.
- **Intentions**: create each folder `0700` (the owner can fill it, never wider than the source while it fills); in the
  post-order pass apply xattrs (tags, comments, FinderInfo), one dates call with birth, then the exact mode, then ACL
  and flags last. Same `created_dirs`-only and success-only rules as today. The pass runs DEEPEST FIRST (today's runs in
  scan order).
- **Landmines**:
  - A `0500` or Locked source folder gets its mode and flags after its children land, never at creation, or the copy
    can't fill it. A parent ACL with `deny add_subdirectory` would block later siblings, so ACLs/flags go in the final
    pass, deepest first.
  - Cross-FS move: a staged folder must keep owner write and carry no ACL or flags until Phase 3 is done. A `0500`
    folder can't be renamed to another parent (`EACCES`), and a merge renames children OUT of staged folders, which a
    Locked, `0500`, or `deny delete_child` folder refuses (all verified). So for moves, mode, ACL, and flags all go in
    M1's deferred pass; xattrs and dates may go on in staging (Phase 3's rename keeps them).
  - Setting a folder's xattrs or dates after a child lands bumps nothing on APFS/HFS+ or exFAT (verified for exFAT in
    review round 1), but the pass still goes deepest first so a future AppleDouble-backed destination can't bump a
    parent that's already dated.
- **Test plan**: M0 folder cells flip green; a `0700` folder lands `0700` with a `022` umask; a tagged folder with a
  custom icon (FinderInfo + `Icon\r`) lands with both; a Locked `0500` folder with children lands Locked and full, by
  copy AND by cross-FS move; cancel leaves folders undated (today's pin stays).
- **DONE**: folder rows of the matrix green; `volume/DETAILS.md` § "Copies keep the source's date" retitled and
  rewritten as "Copies keep the source's metadata".

### M5a: Creation dates cross volumes (the trait carries metadata; SMB first). Size: M (1–2 sessions)

- **Scope**, traced against the code (round 3):
  - `cmdr-fs`: `MetadataClasses`, `VolumeReadStream::metadata(wanted) -> CarriedMetadata` (default empty, beside
    `modified_at` at `read_stream.rs:43`), and `Volume::set_modified` (`volume/mod.rs:494`) → `Volume::apply_metadata`
    (§ "Design sketch"). Implementers to migrate in the same commit: local (`local_posix.rs:485`), SMB
    (`cmdr-smb/src/volume/volume_impl.rs:298`), SFTP (`cmdr-sftp/src/volume/volume_impl.rs:257`), `InMemoryVolume`
    (`in_memory/volume_impl.rs:236`, which also stores and reports every class: it's the reference). Callers to migrate:
    `folder_dates.rs:60` (production) and `conformance/modification_date.rs:188`.
  - Streams that must report or forward `metadata()`: the engine's `CheckpointStream` (`T/checkpoint_stream.rs:121`,
    wraps every `stream_pipe_file` source at `strategy.rs:409`), `ChannelReadStream` (`cmdr-fs`, gains a builder like
    `with_modified_at` at `channel_stream.rs:52`), the local, archive (`volume_streams.rs`, both stream types), git
    (`cmdr-git/src/read_blob.rs`), ADB, and SMB streams. Every other `impl VolumeReadStream` (grep lists ~35, most in
    tests) takes the default; decide each one.
  - The local volume as a destination (`local_posix/streams.rs:156`, `write_from_stream_impl`): asks the stream for the
    classes it stores, applies them through `apply_staged` on the open handle after the last byte, replacing today's
    `File::set_modified` (`:249-263`), still before `sync_data`. As a source (`:93-113`): reports `created`, mode, and
    flags from the `metadata` its open already took, and xattrs lazily from the fd.
  - The engine: `apply_source_mode` and its two call sites (`strategy.rs:557`, `sequential_extract.rs:186`) go; M2's
    quarantine moves into the decorator stream and `FolderMetadata` (§ "Design sketch"); `folder_dates.rs` becomes
    `FolderMetadata`, whose set comes from the listing entry the walk already holds (`FileEntry::created_at`, whole
    seconds like `modified_at`; mode). The post-landing flags subset is snapshotted off the stream before
    `write_from_stream` consumes it and applied with `apply_metadata` only when non-empty.
  - SMB: the streaming read reports `created` from `FileDownload::info()` (`streams.rs:278` reads only `.modified`
    today); the one-frame read builds `InlineReadStream` at `volume_impl.rs:482-487` from the same `FileInfo`, so it
    gains `created` too (the scan pool's `InlineReadStream::new(data, None)` at `scan_pool.rs:548` is enrichment, never
    a copy: leave it). `source_times` (`streams.rs:184`) adds `set_created`, which both writers already send (the handle
    stamp `:190` and the one-frame path `set_times` `:556`). Folders: `apply_metadata` sends CreationTime with
    LastWriteTime in the one compound `set_modified_impl` sends today (`mutation.rs:73-86`). No smb2 change: smb2 0.27.2
    has `FileTimes::set_created`, `FileDownload::info()`, and `read_file_compound_sized_with_info` (checked in the
    vendored registry source).
- **Intentions**: the user's ask, across volumes: SMB ↔ local and SMB → SMB keep creation and modification dates on
  files and folders, with zero new frames. The vocabulary every later milestone fills in. No loss reporting yet (M5b).
- **Landmines**:
  - The test wrapper volumes forward through `forward_volume_methods!` (`faulty_volume_test_support.rs:54`): add the new
    method there, or every fault-injection test silently takes the default. A wrapper STREAM forwards `metadata()` like
    it forwards `modified_at()`.
  - Mode reporters: local, archive, git, AND ADB (§ "Design sketch"). Drop ADB and an ADB → local copy loses the mode
    `apply_source_mode`'s probe gives it today.
  - Rule 5 and the fold: create the local temp `0600` only when the stream reports a mode, and hand `landed_mode` the
    umask-derived default, never the temp's mode (rule 5's ❗). Without a source mode (SMB, MTP), create it as today, or
    every SMB → local file lands `0600`.
  - Post-landing goes on the FINAL name: after `staged.commit`, and on an Overwrite after `finalize_safe_replace`, never
    on the temp. `stream_pipe_file` doesn't finalize; its callers do (`copy_concurrent_task.rs:280`,
    `copy_serial.rs:529`), so it returns the snapshotted subset and they apply it after their finalize.
    `sequential_extract.rs` finalizes inline, after its commit.
  - SMB's temp rename keeps CreationTime (FileRenameInformation doesn't touch `FILE_BASIC_INFORMATION`), and Samba
    stores birth in its `user.DOSATTRIB` xattr (`store dos attributes`, default on). Confirm on the Docker guest share
    (`test/smb-servers/.compose/smb-consumer-guest`, the writable one) that a written CreationTime reads back after the
    rename, and record the evidence anchor in `cmdr-smb`'s `DETAILS.md`.
  - Optional: the one-frame write's date stamp folded into its compound (§ "Resource cost", verify first).
  - `module-cycles` and `index-crate-isolation` fence what `cmdr-fs` may grow; xattr bytes never go into IPC events.
  - Coordinate with issue #289 (tags on SMB): `apply_metadata` with an xattrs-only `CarriedMetadata` IS its
    `write_xattr`; reading many tags at once is listing-shaped and stays #289's.
- **Test plan** (red first: each engine cell fails today, M0's SMB pins flip):
  - `cmdr_fs::volume::conformance::metadata` (beside `modification_date.rs`): each volume's write-then-read of the
    classes it's handed. Until M5b's caps exist, each backend's one-line call passes its classes explicitly (local:
    every class; `InMemoryVolume`: every class; SMB: `created`); M5b swaps the argument for `metadata_caps()`.
  - Engine cells: SMB → local and local → SMB keep birth on a file (streamed AND one-frame) and on a folder; SMB → SMB
    on the guest share; a `0755` script out of a zip and an ADB file (the virtual double) land executable with no source
    stat; a `0600` source from ADB lands `0600` and an SMB file lands at the umask default; M2's quarantine cells stay
    green.
- **Lanes**: `pnpm check rust-tests clippy cfg-gate module-cycles`, `pnpm check rust-integration-tests` (the SMB cells),
  `pnpm check clippy-linux rust-tests-linux`.
- **DONE**: the cells above are green; `apply_source_mode` is gone; docs updated (`volume/DETAILS.md` § "Copies keep the
  source's metadata", `cmdr-fs` volume docs, `cmdr-smb` docs). The report asks David for the Finder check: through a
  fresh mount, Date Created shows the original on files and folders Cmdr copied to a share, and a copy back keeps it.

### M5b: The user hears about what couldn't be kept. Size: M (1 session)

- **Scope**: `MetadataCaps` as a `Volume` predicate (folding `reports_posix_mode`), filled for every backend from
  today's behavior; summing `MetadataReport`s into per-class counters on `WriteOperationState`; the `WriteCompleteEvent`
  field; the queue-row and toast line (FE, i18n keys with `@key` descriptions, the wording marked as David's
  placeholder, § "Telling the user").
- **Intentions**: radical transparency for every destination that can't hold a class, before M6–M9 shrink the list.
- **Landmines**: "had a value" must be free (§ "Telling the user"); a remote source's unknown is never counted. The
  English goes in; translations come from the translator agent, never this one. The FE only renders: classes and counts
  arrive as typed data, the wording lives in i18n.
- **Test plan**: a caps-derived matrix test (§ "The conformance matrix"); M5a's per-volume conformance calls swap their
  explicit class lists for `metadata_caps()`, so a backend can't claim a cap it doesn't honor; a FE unit test for the
  note's rendering; an engine cell where a local → SFTP copy of a tagged file reports the tag loss, and a copy of an
  untagged file reports nothing.
- **DONE**: a local → SFTP copy of a tagged file shows the note; a local → local copy never does.

### M6: SMB keeps Hidden and Locked. Size: S in smb2 + S–M in Cmdr (1 session)

- **Scope**: smb2: let `set_times`/`set_handle_times` send `FileAttributes` (0 still means "don't change"), and surface
  attributes on `DirectoryEntry`/`FileInfo` (the CREATE response and the listing already carry them on the wire). Cmdr:
  the SMB read stream reports `Hidden`/`Locked` from `FileDownload::info()`; the writer sends `HIDDEN` in the SET_INFO
  that already carries the times; `READONLY` via `apply_metadata` after landing; folders by path; caps updated.
- **Intentions**: both directions, zero extra round trips on reads, the same one frame on writes, one extra path
  compound only for a Locked file.
- **Landmines**: `FILE_ATTRIBUTE_READONLY` is a rename-blocker candidate (Samba maps it to the mode's write bit), so it
  goes in `apply_metadata` after landing per rule 3, and `HIDDEN` rides the temp's SET_INFO. The volume engine's
  in-flight rollback (`T/volume/cleanup.rs`) and the journal rollback delete landed files on the share: a `READONLY` one
  refuses deletion (`STATUS_CANNOT_DELETE`), so rule 9 applies here too (clear the attribute on a verified item, then
  delete). Setting attributes to 0 means "don't change", so "clear hidden" needs `FILE_ATTRIBUTE_NORMAL`. smb2 is
  David's crate and exempt from the 3-day rule, but it releases first and Cmdr re-vendors the fixtures
  (`apps/desktop/test/smb-servers/.compose/VENDORED.md`).
- **Test plan**: matrix cells for SMB both ways on the Docker fixture, including a rollback that removes a Locked copy;
  smb2's own integration test against the QNAP (read-only checks only: ❌ never write to David's NAS without asking).
- **DONE**: Finder, through a fresh mount, shows Hidden and Locked on files and folders Cmdr copied to the share, and a
  copy back keeps them.

### M7: SMB keeps Finder tags, comments, icons, and resource forks. Size: L (blocked)

- **Scope**: blocked on vdavid/smb2#9 (named streams, macOS xattr mapping, bulk API) and shares the trait capability
  with #289. Read the source's xattrs from the CREATE + stream enumeration it can compound; write them as streams on the
  temp before the rename.
- **Intentions**: what smbfs writes (`AFP_AfpInfo`, `AFP_Resource`, `com.apple.metadata<U+F022>_kMDItemUserTags`) so
  Finder on any Mac sees it.
- **Landmines**: a server without `vfs_fruit`/streams refuses streams: the caps must be probed per share, not per
  backend, and a refusal counts as a reported loss. Stream names carry the U+F022 mapping (issue #289's verified QNAP
  finding). The Docker fixture has no `fruit` share today; one comes with smb2#9.
- **Test plan**: matrix cells against the new `vfs_fruit` fixture; manual QA on the QNAP (read-only side) and a Synology
  if one is reachable.
- **DONE**: a tagged file copied local → SMB → local keeps its tags, and Finder sees them on the share.

### M8: Archives: AppleDouble, pax xattrs, and `__MACOSX` on compress. Size: M (1 session)

- **Scope**: extract (`cmdr-archive`'s index and read streams, `sequential_extract.rs`): apply `__MACOSX/._name`
  AppleDouble entries as xattrs on their file and folder, and don't extract `__MACOSX` itself; tar `SCHILY.xattr.*` and
  a tar's own `._name` entries. A move out of a zip (`archive_edit/move_out.rs`) also deletes each moved item's sidecar
  in its `{ delete }` rewrite, or the zip keeps orphans. Compress, both writers (`fresh_zip.rs` and the mutator behind
  `archive_edit/copy_into.rs`): write `__MACOSX/._name` entries ONLY when at least one source has xattrs worth keeping
  (David's pick: not `provenance`, not quarantine, not an empty FinderInfo), so an ordinary zip stays clean for Windows
  users.
- **Intentions**: match `ditto -x`/`ditto -c -k --sequesterRsrc` and bsdtar (verified behavior).
- **The seam**:
  - Files: the archive volume's read stream reports the paired xattrs through M5a's `metadata(wanted)`, and the local
    destination puts them on the temp like any other source's. No archive-specific write path.
  - Folders: the walk has no stream for a folder, so M8 adds the one extra read the design sketch allows,
    `Volume::folder_metadata(path, wanted) -> CarriedMetadata`, default empty (no I/O for every other backend, rule 6).
    Only the archive volume answers, from its in-memory index plus one member read for a folder that has a sidecar.
    `FolderMetadata` merges it with the listing's set.
  - Hiding: the archive index drops `__MACOSX/` and pairs each sidecar with its item, so listing, the copy scan's counts
    and bytes, and the walk never see it (§ "Decided").
- **Pairing, by format** (traced in round 3):
  - **zip is random-access** (`ArchiveFormat::is_sequential` is false; `strategy_sequential_tests.rs:290`): it extracts
    through `stream_pipe_file` and the archive volume's `open_read_stream`, so the stream looks its sidecar up in the
    index at open and reads it before the file's first byte. `ditto` writing `._name` AFTER `name` doesn't matter here.
    On a remote archive that's one extra ranged read per file THAT HAS a sidecar, never for plain files.
  - **Compressed tar is sequential** (one forward decode pass, `sequential_extract.rs`): a pax header precedes its
    entry, so `SCHILY.xattr.*` is in hand before the bytes. A macOS `tar -c` writes `._name` entries too, which we
    expect BEFORE `name`: verify with a fixture made by `/usr/bin/tar -czf` before relying on it. If a sidecar ever
    follows its file, it's dropped and counted, ❌ never applied after landing (a path xattr write on a `0444` file is
    `EACCES`, and it breaks rule 2's order). Buffer a sidecar until its file arrives, within the existing DoS caps.
- **Landmines**:
  - AppleDouble is untrusted input: parse it defensively (bounded sizes, the existing DoS caps), never let a hostile
    `._` name escape (it goes through the same sanitizer).
  - The "worth keeping" test on compress costs a name-only `flistxattr` per source on the fd it's reading anyway; never
    a second pass over the sources (rule 6 in spirit).
- **Test plan**: fixtures made with `ditto -c -k --sequesterRsrc` and `/usr/bin/tar -czf` (checked in, tiny; a tagged
  file AND a tagged folder in each), a hostile AppleDouble cell, a move-out cell (the zip keeps no orphan sidecar), a
  compress cell with a tagged source (has `__MACOSX`) and one with plain sources (has none).
- **DONE**: extracting a Finder-made zip yields files with their tags and no `__MACOSX`; compressing a tagged file and
  extracting it with `ditto -x -k` keeps the tags.

### M9: The rest: SFTP, S3, WebDAV, MTP, ADB. Size: M–L (1–2 sessions, spikes first)

- **Scope**:
  - SFTP: report mode from the attrs we already get (it's free); write it with `SETSTAT` permissions, folded like
    `landed_mode` (never wider, no setuid). Birth n/a.
  - S3: write and read rclone's `btime` and `mode` keys beside `mtime` (same format rclone uses). Server copy and rename
    already carry them (§ "Cross-volume").
  - WebDAV: spike `X-OC-CTime` on the Nextcloud fixture; adopt if it sticks, else record n/a.
  - MTP: spike whether Android keeps `DateCreated` from `SendObjectInfo`; adopt or record n/a. ❗ The virtual MTP double
    can't answer this (it stores what it's sent): it needs a real phone, so the agent writes the code path behind the
    spike and the report asks David to run it with his phone plugged in.
  - ADB: send the folded source mode in `SEND` instead of a fixed `0660` (low value, cheap).
- **Landmines**: every new source report must ride the existing open or listing (rule 6). S3 metadata counts toward the
  2 KB user-metadata limit. A spike that fails is a result: record it in the backend's `DETAILS.md` with an evidence
  anchor.
- **Test plan**: each backend joins the per-volume conformance layer (§ "The conformance matrix") on its existing Docker
  fixture or double, its caps updated so M5b's derived expectations follow; plus an S3 server-copy cell that keeps
  `btime`. Each backend is its own crate and its own commit, so two agents can take two backends at once.
- **DONE**: every matrix cell is Kept or Reported, no cell is a silent drop.

### M10: Close-out. Size: S (1 session)

- An adversarial conformance review against the invariants below, a docs audit reading every commit body, the
  `docs/notes` evidence linked from the right `DETAILS.md`, a follow-up issue (tagged `claude-created`) for the Locked
  same-drive move M0 pinned, and this spec wiped per `docs/specs/DETAILS.md`.

## The conformance matrix

Metadata flows source stream → `CarriedMetadata` → destination, with nothing pair-specific in between, so the matrix
FACTORS. A full (source × destination × class) grid against real fixtures would be ~300 Docker-backed copies proving the
same thing many times; instead, three layers:

- **Per volume** (`cmdr_fs::volume::conformance::metadata`, beside `modification_date.rs`, M5a): each volume's READ half
  (its stream reports every class its caps say it reports, matching its listing) and WRITE half (a synthetic stream
  carrying every class lands each class its caps say it stores, within its resolution: FAT 2 s, SMB 100 ns, whole
  seconds elsewhere). So a backend can't claim a cap it doesn't honor. One line per backend, like
  `sftp_transfer_integration_test.rs:186-188` today. M5a passes each backend's classes explicitly; M5b derives them from
  `metadata_caps()`.
- **Caps-derived expectations** (M5b): `(source caps, dest caps, class) → Expect::{Kept, Reported, NotApplicable}`,
  computed, never hand-written, plus a short exception list (each with a reason) for a phase's honest gaps. A
  hand-written `Kept` that caps say is impossible fails the build. Asserted through the engine with `InMemoryVolume`
  doubles configured to each backend's caps, which checks the engine's routing and the loss report for every pair at
  in-memory speed.
- **A few real engine pairs** where the engine does something pair-specific: local ↔ SMB (streamed and one-frame), SMB →
  SMB, local → SFTP (the loss note), archive → local (quarantine, AppleDouble), the cross-FS move, and the native matrix
  of M0. The virtual MTP and ADB doubles (`backend_suites/mtp_dates_test.rs`, `adb_transfer_test.rs`) join the
  per-volume layer.
- **Backends**: local (APFS temp dir; HFS+ via `disk_images` in the ignored real-image lane), `InMemoryVolume` (the
  reference implementation of every class), and every Docker fixture that exists (SMB, SFTP, WebDAV, S3).
- **The FAT/exFAT gap** is real and accepted (rule 8): exFAT and FAT32 have no automated cell. What's pinned instead is
  the notes file (birth settable, `._` sidecars, resource forks refused, temp sidecar and birth surviving the landing
  rename on exFAT) and M3's manual QA step. A Locked/Hidden ↔ FAT attribute regression would only show up there.

## Invariants

1. A metadata failure never fails a copy, and is never silent: it's counted and reported (rules 1, § "Telling the
   user"). Every apply function returns a `MetadataReport` and has no error path.
2. Dates are set after every mtime-bumping step, in one call, on the temp before landing; the landing rename carries
   them. xattrs go before mode.
3. Nothing that will still be renamed (a staged temp, a cross-FS move's staging tree before Phase 3) carries a
   rename-blocking flag, ACL, or (for folders) a mode without owner write. Post-landing order: ACL, then flags.
4. Only created items get metadata; a merged-into folder keeps its own.
5. Mode never widens and never escalates on a routed path, and never sits wider than the source mid-copy.
6. No per-file round trip is added to a remote source.
7. Every matrix cell is Kept, Reported, or NotApplicable by caps; none is a silent drop.
8. The temp's NAME and the staging decision are untouched by this work.
9. Every reversal and the in-flight sweep remove or restore what this work makes, Locked and ACL'd items included, after
   the check each path already makes.
10. A plain file (no flags, no ACL, no xattrs worth keeping) costs no more syscalls or frames than before this work.
11. The ordering lives in ONE host helper in `cmdr-fs`; no backend or engine path reimplements it. Every macOS-only call
    is `cfg`-gated and the Linux lanes stay green.

## Decided (David, 2026-10-11)

- **Extract quarantine** ships early as its own small milestone (M2).
- **Compress** writes `__MACOSX` only when the sources have xattrs worth keeping (option (c), M8).
- **The Locked-file landing bug** ships early, right after M0 confirms it (M1).
- **The loss-note wording** is a placeholder David rewrites; it stays marked as one (§ "Telling the user").
- **Undo removes a Locked copy**: the reversal unlocks (and strips a `deny delete` ACL from) our own identity-verified
  copy, then deletes it. An item that fails verification is left alone and reported as a typed skip (M1, rule 9).
- **A Locked item that fails to move within one drive** (`rename` `EPERM`) stays out of scope: M0 pins it, and M10 files
  it as a follow-up issue.
- **Browsing a zip hides `__MACOSX`** (M8): once its sidecars ride their files as metadata, the archive index drops the
  folder from listings too, matching extract, `ditto -x`, and Archive Utility.

## Open questions for David

1. **Finder as the reference**: could you check once, by hand, whether Finder keeps "Date Created" and Locked when
   copying a file and a folder to a USB stick? Our own tests can't drive Finder without a TCC prompt. It only sets
   expectations; the plan keeps birth either way.
