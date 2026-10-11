# Transfer metadata preservation (plan)

**Problem.** A copy or move in Cmdr keeps a file's bytes and its modification date, and quietly drops much of the rest:
creation dates, Finder tags and comments on folders, a folder's permissions, Locked and Hidden flags, and, across most
backends, everything but mtime. A move makes the loss permanent, since the original is deleted. A user asked for
"original timestamps, similar to `ditto`" (Lars, macOS 15.8.1, Cmdr 0.51.0); David's goal is wider: **no more lost
metadata**. `ditto` parity is the floor, not the ceiling, and our own measurements show `ditto` itself drops creation
dates across volumes (`docs/notes/transfer-metadata-experiments-2026-10-11.md`).

**Status**: planned, not started (2026-10-11). Issue: none yet. Evidence: the notes file above, plus the file:line
citations in § "Fresh read". Sizes are rough agent-sessions (one session ≈ one focused milestone agent run).

## Loud rules

1. ❌ **Never fail a copy over metadata.** The bytes are the copy. Every metadata step is best effort: a refusal is
   counted (§ "Telling the user") and logged, never an error. The one exception is the landing itself (rule 3).
2. ❌ **Dates go LAST, in one call.** Set birth and mtime (and atime where we set it) in ONE `setattrlist` /
   `File::set_times`, after every xattr. A resource-fork write bumps mtime, and on APFS/HFS+ an mtime earlier than the
   birth time drags birth down to it (verified, notes § "The quirks").
3. ❌ **Nothing that blocks a rename goes on a staged temp.** `uchg`/`schg`/`uappnd` flags and ACLs (a `deny delete`
   entry blocks `rename(2)`) apply to the LANDED file, after the rename. Mode, xattrs, and dates go on the temp, as
   today. This also keeps every `.cmdr-tmp-*` removable by the sweep.
4. ❌ **Only metadata of things the copy CREATED.** A folder the copy merged into keeps its own metadata (today's rule
   for dates, `volume/DETAILS.md` § "Copies keep the source's date"). An Overwrite replaces the file, so the landed file
   wears the SOURCE's metadata, the way Finder's Replace does.
5. ❌ **Never widen, never escalate.** Mode follows the `landed_mode` fold (umask-respecting) for routed volumes;
   setuid, setgid, and sticky travel only local-to-local; `SF_*` system flags and ownership never travel (non-root
   can't, and we don't want to when elevated either, unless a later spec decides so).
6. ❌ **No new per-file round trips on remote sources.** Metadata rides what the open or the listing already returned
   (the `modified_at` discipline). A class that would cost a request per file is gated on a capability, like
   `reports_posix_mode` gates the mode probe today.
7. ❌ **Don't touch the staging temp NAMING.** A parallel effort is changing its length
   (`crates/cmdr-fs/src/staging.rs`, `volume/finalize.rs`). This plan only changes what goes on the temp and when.
8. ❌ **No FAT or exFAT images in tests.** `cmdr_fs::testing::disk_images` supports APFS and HFS+ only, for a
   kernel-panic reason (`crates/cmdr-index/src/indexing/tests/CLAUDE.md`). FAT/exFAT behavior is pinned by the notes
   file and a manual QA step, never a test.

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
  (extract new, compress per open question 3). Everywhere else impossible and reported. Skip `com.apple.provenance`.
- **ACLs**: local-to-local, post-landing (moved there from the temp, which is a bug fix).
- **BSD flags**: `hidden`, `uchg` (Locked), `uappnd`, `nodump` local-to-local, post-landing (new on the chunked path);
  `hidden` ↔ SMB `FILE_ATTRIBUTE_HIDDEN` and Locked ↔ `FILE_ATTRIBUTE_READONLY` (new, needs a small smb2 change).
- **Symlinks**: stay links locally (today), plus the link's own dates (new, `FSOPT_NOFOLLOW`). Remote: SMB has none to
  give (the server resolves them); SFTP could (`SYMLINK`), and is out of scope here.
- **Folders**: everything a file gets, on folders the copy created, after their contents land.
- **Ownership**: never (rule 5).

## Fresh read of the current code

Paths are from the repo root; `T` = `apps/desktop/src-tauri/src/file_system/write_operations/transfer`.

### Local to local (the native engine)

- **Strategy pick** (`T/copy_strategy.rs:165-187`): macOS same-APFS-volume → `AppleClone`, everything else (cross-volume
  APFS, HFS+, exFAT, FAT, the macOS SMB mount) → `Chunked`. Linux: `KernelCopyRange`, or `Chunked` on a network mount.
- **Every arm lands through `stage_and_land_file`** (`T/copy_strategy.rs:207-269` → `write_operations/overwrite.rs:67`):
  fill a `.cmdr-tmp-*` sibling, set the existing destination aside when replacing, one rename.
- **`AppleClone`** (`T/macos_copy.rs:279-386`, flags at :289):
  `copyfile(COPYFILE_ALL | COPYFILE_CLONE | NOFOLLOW_SRC | EXCL)` onto the TEMP. A clone keeps everything, birth
  included (verified). ❗ But `COPYFILE_STAT` puts `uchg` on the temp and `COPYFILE_ACL` puts a `deny delete` ACL on it,
  and either one makes the landing rename fail (reproduced with the same syscalls; notes § "The quirks"). Expected
  today: copying a Locked file within one APFS volume fails and leaves a `uchg` temp the sweep can't `unlink`. M0 pins
  this as red first.
- **`Chunked`** (`T/chunked_copy.rs:65-101`): data, `sync_data`, then `copy_metadata` (`:280-294`): xattrs (`:297-336`,
  every name the `xattr` crate lists, failures per name are `debug!`), ACLs via `exacl` (`:340-358`, onto the TEMP, so
  the same rename hazard), `copy_timestamps` (`:367-382`, `filetime::set_file_times(atime, mtime)`, no birth), then
  `copy_permissions` (`:385-397`, full mode incl. setuid). No BSD flags. Birth on APFS/HFS+ ends up == source mtime by
  the quirk; on exFAT/FAT it's "now" (verified). `transfer/DETAILS.md:1185` claims birthtime "matters only on
  same-volume copies"; that's wrong and gets corrected in M1.
- **Linux `KernelCopyRange`** (`T/linux_copy.rs:116-140`): mode and atime/mtime only, no xattrs, no ACLs. Linux can't
  set birth time at all.
- **Symlinks** (`T/macos_copy.rs:411-423`, used from `T/copy/single_item.rs`): `read_link` + `symlink`, a fresh link.
  Its own dates are "now" and its own xattrs are dropped.
- **Folders**: created bare, as FILE parents (`T/copy/single_item.rs:362`, `create_dir_all`), by the empty-folder pass
  (`T/copy/scanned_dirs.rs:85`), or eagerly for a self-duplicate (`T/copy/mod.rs:286`). After the file loop,
  `date_created_dirs_like_their_sources` (`T/copy/scanned_dirs.rs:109-141`) sets ONLY mtime, only on `created_dirs`,
  only on success. Everything else on a folder is lost: birth (== mtime by the quirk), mode (a `0700` folder lands
  `0755`: a privacy regression), tags, comments, FinderInfo (custom folder icons), ACLs, `hidden`, Locked.
- **Same-FS move** (`T/move_op/same_fs.rs`): `rename(2)`, keeps everything.
- **Cross-FS move** (`T/move_op/cross_fs.rs`): stages into a folder on the destination (`:118`), copies each file
  through `copy_single_item` (`:210`, so the `Chunked` gaps above), dates the staged folders (`:310`), renames into
  place, then deletes the source ledger. Every gap above becomes permanent loss here. ❗ Phase 3's `OverwriteOlder`
  compares the STAGED copy's mtime with the destination's (`T/linux_copy.rs:128-131` explains), so mtime on the staged
  copy is load-bearing for conflict logic.

### Cross-volume (the volume engine)

- **Contract**: `T/volume/DETAILS.md:806-905` § "Copies keep the source's date". The source reports
  `VolumeReadStream::modified_at` (`crates/cmdr-fs/src/volume/read_stream.rs:43`, required); the destination writes it
  inside `write_from_stream`. Explicitly out: birth time and permissions (`:824`).
- **Mode**: `T/volume/landed_mode.rs:46` folds the source's mode onto a LOCAL destination's temp before the rename,
  gated on `Volume::reports_posix_mode` (`crates/cmdr-fs/src/volume/mod.rs:956`; true for local, archive, git, ADB).
  `T/volume/DETAILS.md:740-804`.
- **Folder dates**: `T/volume/folder_dates.rs:44-74`, through `Volume::set_modified`
  (`crates/cmdr-fs/src/volume/mod.rs:494`, defaults to `NotSupported`; the first `NotSupported` ends the pass).
- **Trait surface**: no birth, xattr, flag, ACL, or symlink-creation method. `FileEntry` carries `modified_at`,
  `created_at`, `added_at`, `opened_at`, `permissions` (0 = no mode), read-only Finder tags.
- **Local as a destination** (`apps/desktop/src-tauri/src/file_system/volume/backends/local_posix/streams.rs:249-263`):
  `File::set_modified` on the handle after the last byte. No birth, so SMB → local lands birth == mtime.
- **SMB** (`crates/cmdr-smb`, smb2 0.27.2):
  - Writes `LastWriteTime` only (`crates/cmdr-smb/src/volume/streams.rs:184-196`, and `:553-561` for the one-frame
    write); folders by path (`crates/cmdr-smb/src/volume/mutation.rs:70-86`).
  - smb2 already has `set_created` (`smb2/crates/smb2/src/client/times.rs:93`); `FileAttributes` always goes as 0
    (`times.rs:124-128`), so DOS attributes can't be set yet. New files are `FILE_ATTRIBUTE_NORMAL`.
  - Reads: `DirectoryEntry` and `FileInfo` carry `created` (`smb2/.../client/tree.rs:144-155, 224-235`), and Cmdr maps
    it to `FileEntry::created_at` (`crates/cmdr-smb/src/volume/mapping.rs:30-31`); attributes are read only to decide
    `is_directory` (`tree.rs:3296`). The read stream carries only `modified_at`.
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
  `mode`, `atime` keys are neither written nor read. No folder dates (prefixes).
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
  and Archive Utility don't allow (verified for the first two). That's a Gatekeeper gap, not only lost metadata.
- **Compress** (zip only): DOS time + `UT` mtime and unix mode per entry
  (`crates/cmdr-archive/src/mutation/mutator.rs:440-480`, `fresh_zip.rs:432-438`); symlinks and special files skipped
  (`fresh_plan.rs`); no xattrs, no `__MACOSX`.
- **Editing a zip in place** keeps the ARCHIVE file's own metadata with `copyfile(STAT|ACL|XATTR)`
  (`crates/cmdr-archive/src/mutation/DETAILS.md:52-56`). Its claim that `COPYFILE_STAT` carries birth time is wrong for
  a non-clone copy (verified: it doesn't), so a zip edit resets the archive's birth to its mtime. Fixed in M1 with the
  shared helper.

## Gap matrix (today → target)

"keep" = preserved today; "LOST" = the destination can hold it and we drop it; "n/a" = the destination can't hold it.

- **Local, same APFS volume (clone)**: files keep everything, but Locked or `deny delete` files FAIL to land (bug).
  Symlink dates LOST. Folders: birth, mode, xattrs, ACL, flags LOST.
- **Local, other APFS / HFS+ volume (chunked)**: file birth LOST (shows mtime), flags LOST, ACL kept but on the temp
  (rename hazard), xattrs/mode/mtime/atime keep. Folders as above. Symlink dates LOST.
- **Local, exFAT / FAT**: file birth LOST (shows now), `hidden`/Locked LOST (FAT attributes can hold them), xattrs keep
  as `._` sidecars, resource forks n/a (the FSKit drivers refuse them, notes § "Results"), ACLs/mode n/a.
- **Local, macOS SMB mount (chunked through smbfs)**: as other-volume; birth should be settable through smbfs (to verify
  in M1's manual QA), xattrs depend on the server's `vfs_fruit`.
- **Same-FS move**: keeps everything. **Cross-FS local move**: the chunked row, made permanent.
- **Local → SMB (direct)**: mtime keep; birth LOST (`CreationTime` = now); hidden/Locked LOST (needs smb2 attributes);
  xattrs LOST (needs smb2#9 + a server with streams); mode/ACL n/a. Folders: mtime keep, the rest as files.
- **SMB → local**: mtime keep; birth LOST (the CREATE response and listing already carry it); hidden/readonly LOST;
  xattrs LOST (smb2#9).
- **SMB → SMB, other share**: streamed, mtime only; birth LOST. Same share: rename keeps all.
- **Local ↔ SFTP**: mtime keep; birth n/a; mode LOST both ways (attrs carry `permissions`); xattrs n/a.
- **Local ↔ WebDAV**: mtime keep; birth LOST toward Nextcloud only if `X-OC-CTime` works (to verify), else n/a; folder
  dates n/a; the rest n/a.
- **Local ↔ S3**: mtime keep; birth and mode LOST (rclone keys exist); folders n/a.
- **Local ↔ MTP**: mtime keep; birth to verify (`DateCreated` is a field Android likely ignores), folder dates n/a.
- **Local ↔ ADB**: mtime keep; mode sends a fixed `0660` (low value: `/sdcard` ignores modes); birth and folder dates
  n/a.
- **Archive → local**: mtime and mode keep; xattrs from `__MACOSX` LOST; quarantine LOST (security); tar pax xattrs
  LOST; folder dates keep.
- **Local → zip**: mtime and mode keep; xattrs LOST (open question 3); birth n/a in zip.

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
  line naming the classes and counts. Draft wording for David to rewrite: "Copied 214 files. This server can't store
  creation dates or Finder tags, so those stayed behind." i18n through the translator agent, never the coding agent.
- **How it's computed**: the destination's `MetadataCaps` (M3) says what it can hold; the engine counts, per class,
  source items that had a value and weren't kept (caps say no, or a best-effort write was refused). The counts ride the
  `WriteCompleteEvent`.

## Design sketch (decided in M3, sketched here so M1/M2 don't paint into a corner)

- **`CarriedMetadata`** (in `cmdr-fs`): `modified`, `created`, `accessed`, `mode`, `flags` (a small typed set: `Hidden`,
  `Locked`, `AppendOnly`, `NoDump`), and `xattrs: Vec<(String, Vec<u8>)>` with a size cap (resource forks can be large;
  above the cap, drop and count). The read stream exposes it (`VolumeReadStream::metadata()`, defaulting to
  `{ modified: modified_at(), ..None }` so every backend keeps compiling), filled from what the open already learned.
- **`MetadataCaps`** per volume, replacing ad hoc predicates (`reports_posix_mode` folds into it): which classes it can
  STORE, which it can REPORT cheaply, and whether folders take them.
- **Writing**: the destination applies the carried set inside `write_from_stream` in the rule-2 order (xattrs, mode,
  dates in one call), as today's dates. Rename-blocking classes (rule 3) go through a new
  `Volume::finish_landed(path, &CarriedMetadata)`, default no-op, called by the engine right after the landing rename.
- **Folders**: `Volume::set_modified` widens to `apply_folder_metadata(path, &CarriedMetadata)`; `FolderDates` becomes
  `FolderMetadata` and carries the whole set, still stamped post-order after the subtree lands.
- **Local to local stays native.** It doesn't go through the trait; M1 and M2 share one helper module (say
  `transfer/metadata/`) that both the chunked path and the local volume's `write_from_stream` call, so there's one
  implementation of the ordering.

## Milestones

### M0: Characterization (pin today, red where it's a bug). Size: M (1 session, ~600 test lines)

- **Scope**: tests only. A native-engine matrix: same-APFS (clone) and APFS → HFS+ image (chunked, via
  `cmdr_fs::testing::disk_images`, `#[ignore]`d into the existing real-image lane), each copying a fixture with every
  class (the notes file's fixture). A cross-FS move cell. An engine cell per backend suite that already exists
  (`write_operations/backend_suites/network_dates_test_support.rs` shape) asserting birth is NOT kept today.
- **Intentions**: pin reality so M1–M8 diff provably. The Locked-file and `deny delete` landing failures pinned as the
  current behavior (the test asserts today's outcome and names the fix milestone), then flipped in M1.
- **Landmines**: don't assert FAT/exFAT (rule 8). `stat` birth reads need
  `std::os::macos::fs::MetadataExt::st_birthtime` or `Metadata::created()`. A test file with `uchg` or a `deny delete`
  ACL must clean itself up (`chflags nouchg`, `chmod -N`) or `TestDir`'s drop fails; add that to the fixture builder,
  not each test. `com.apple.provenance` appears on every written file: exclude it from comparisons.
- **Test plan**: the tests ARE the plan. Verify each pin bites by temporarily breaking the line it guards (red first).
- **DONE**: matrix green on today's code, the two landing bugs reproduced in a test, commit lists every pinned cell.

### M1: Local files keep everything (birth, flags, ACL after landing). Size: M (1–2 sessions)

- **Scope**: `chunked_copy.rs`, `macos_copy.rs`, `copy_strategy.rs`, `overwrite.rs` (a post-landing hook), the new
  shared `transfer/metadata/` helper, symlink dates, Linux xattrs/ACLs on `KernelCopyRange`, the zip-edit archive-file
  metadata (`crates/cmdr-archive/src/mutation/mutator.rs`) through the same helper.
- **Intentions**:
  - Chunked: xattrs, mode, then ONE `File::set_times` with `set_created` (std's `FileTimesExt`, Apple-only) + modified +
    accessed on the open handle, before `sync_data`. Then, after landing: ACL and user flags.
  - Clone: keep `copyfile` for the clone, but strip `uchg`/ACL from what lands on the temp
    (`COPYFILE_DATA|XATTR|CLONE` + our own stat step, or clone then clear and re-apply after landing; the implementer
    picks and pins). Rule 3 for both.
  - Symlinks: the link's own birth/mtime via `setattrlist(FSOPT_NOFOLLOW)`, its xattrs with `XATTR_NOFOLLOW`.
  - Skip `com.apple.provenance` in the xattr copy (check `copyfile`'s own skip behavior, match it).
  - Correct `transfer/DETAILS.md:1185` and `cmdr-archive/src/mutation/DETAILS.md` § "Metadata preservation".
- **Landmines**:
  - Rule 2: one call. `filetime` can't set birth; drop it on macOS for this path.
  - The cross-FS move's `OverwriteOlder` reads the staged copy's mtime: keep mtime on the temp.
  - A post-landing step on an Overwrite runs on the NEW file; the aside is the old one. Don't confuse them.
  - A crash between landing and post-landing leaves a file without its ACL/flags. Accept and document; no ledger entry.
  - `uchg` on a landed file makes a later rollback's delete fail with `EPERM`: the reversal must clear user flags it set
    itself before deleting (it already verifies identity; add the flag clear to the verified path).
  - The `in_flight_sweep` must not be asked to remove a `uchg` temp: rule 3 guarantees it, add an assertion test.
- **Test plan**: M0's matrix flips green for files; new cells: Locked file same-volume and cross-volume lands Locked;
  `deny delete` file lands with its ACL; birth < mtime and birth > mtime both survive; a reversal removes a Locked copy.
  Manual QA (David or the agent): exFAT USB stick and a macOS SMB mount, birth and tags, against the notes file.
- **DONE**: no class lost on local files except ownership; the bug cells are green; docs corrected.

### M2: Local folders keep everything. Size: M (1 session)

- **Scope**: `copy/scanned_dirs.rs` (`date_created_dirs_like_their_sources` → a full folder-metadata pass), folder
  creation sites (`copy/single_item.rs:362`, `scanned_dirs.rs:85`, `copy/mod.rs:286`), cross-FS move's staged folders.
- **Intentions**: create each folder with `source_mode | 0o700` (owner can fill it, never wider than the source); in the
  post-order pass apply xattrs (tags, comments, FinderInfo), exact mode, one dates call with birth, then ACL and flags
  last. Same `created_dirs`-only and success-only rules as today.
- **Landmines**: a `0500` or Locked source folder must be applied AFTER its children land, never at creation, or the
  copy can't fill it. Deepest-first ordering: a Locked child folder doesn't block its parent's metadata, but a parent
  ACL with `deny add_subdirectory` would block later siblings, so ACLs/flags go in the final pass, deepest first. The
  cross-FS move renames STAGED folders into place: a Locked staged folder can't be renamed (rule 3), so for moves the
  flags/ACL pass runs after Phase 3.
- **Test plan**: M0 folder cells flip green; a `0700` folder lands `0700` with a `022` umask; a tagged folder with a
  custom icon (FinderInfo + `Icon\r`) lands with both; a Locked folder with children lands Locked and full; cancel
  leaves folders undated (today's pin stays).
- **DONE**: folder rows of the matrix green; `volume/DETAILS.md` § "Copies keep the source's date" retitled and
  rewritten as "Copies keep the source's metadata".

### M3: The volume trait carries metadata, and the user hears about losses. Size: L (2–3 sessions)

- **Scope**: `CarriedMetadata`, `MetadataCaps`, `VolumeReadStream::metadata()`, `finish_landed`, `apply_folder_metadata`
  (§ "Design sketch"); `landed_mode.rs` folds into it; the local volume writes birth and xattrs as a destination and
  reports them as a source; the loss counter on `WriteOperationState` and its event field; the queue-row and toast line
  (FE, i18n keys with `@key` descriptions).
- **Intentions**: one vocabulary every later milestone fills in. Routed sources (git portal, archives) report what they
  have; local → SMB and friends immediately report "birth not kept" honestly before M4 fixes it.
- **Landmines**: the conformance pattern requires every wrapper volume to forward new methods
  (`forward_volume_methods!`), or they silently take defaults (today's `set_modified` gotcha). A required
  `VolumeReadStream` method is safer than a default for the source half; keep `modified_at` working or migrate its
  20-odd implementers in one commit. The `module-cycles` and `index-crate-isolation` checks fence what `cmdr-fs` may
  grow; the xattr bytes must not end up in IPC events. Coordinate the xattr-method shape with issue #289, which wants
  `read_xattr_many` / `write_xattr` on the trait for tags: one capability, two users.
- **Test plan**: a new `cmdr_fs::volume::conformance::metadata` assertion set; the cross-backend matrix (§ "The
  conformance matrix") with every cell either Kept or Reported; a FE unit test for the note's rendering.
- **DONE**: the matrix exists and is green with today's capabilities; a local → SFTP copy of a tagged file shows the
  note; docs updated (`volume/DETAILS.md`, `cmdr-fs` volume docs).

### M4: SMB keeps creation dates and Hidden/Locked. Size: M (1 session in Cmdr + S in smb2)

- **Scope**: smb2: let `set_times`/`set_handle_times` send `FileAttributes` (non-zero means set), and surface attributes
  on `DirectoryEntry`/`FileInfo` and the CREATE response's times on the read handle. Cmdr: the SMB read stream reports
  `created` and flags from the CREATE response (no extra frame); the writer sends `CreationTime` with `LastWriteTime` in
  the same SET_INFO; folders by path; caps updated.
- **Intentions**: both directions, zero extra round trips on reads, the same one frame on writes.
- **Landmines**: `FILE_ATTRIBUTE_READONLY` on the staged temp: verify a rename still works on Samba, QNAP, and Synology
  (Windows allows renaming a read-only file; Samba maps it to the mode's write bit). If not, it goes in `finish_landed`.
  Setting attributes to 0 means "don't change", so "clear hidden" needs `FILE_ATTRIBUTE_NORMAL`. smb2 is David's crate
  and exempt from the 3-day rule, but it releases first and Cmdr re-vendors the fixtures
  (`apps/desktop/test/smb-servers/.compose/VENDORED.md`).
- **Test plan**: matrix cells for SMB both ways on the Docker fixture; smb2's own integration test against the QNAP
  (read-only checks only: ❌ never write to David's NAS without asking).
- **DONE**: Finder, through a fresh mount, shows the original Date Created and Hidden/Locked on files and folders Cmdr
  copied to the share.

### M5: SMB keeps Finder tags, comments, icons, and resource forks. Size: L (blocked)

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

### M6: Archives: quarantine, AppleDouble, and pax xattrs. Size: M (1 session)

- **Scope**: extract (`sequential_extract.rs`, the archive volume's read streams): propagate the archive file's
  `com.apple.quarantine` to every extracted file and folder; apply `__MACOSX/._name` AppleDouble entries as xattrs on
  their file and don't extract `__MACOSX` itself; tar `SCHILY.xattr.*`. Compress: open question 3.
- **Intentions**: match `ditto -x` and bsdtar (verified behavior). The quarantine part is a security fix and could ship
  on its own ahead of everything else (open question 2).
- **Landmines**: AppleDouble is untrusted input: parse it defensively (bounded sizes, the existing DoS caps), never let
  a hostile `._` name escape (it goes through the same sanitizer). The archive can be on a remote volume: the quarantine
  source is the ARCHIVE's xattr, read once per extract, only when the archive is local.
- **Test plan**: fixtures made with `ditto -c -k --sequesterRsrc` (checked in, tiny), a quarantined zip extract cell, a
  hostile AppleDouble cell.
- **DONE**: extracting a quarantined Finder-made zip yields quarantined files with their tags and no `__MACOSX`.

### M7: The rest: SFTP, S3, WebDAV, MTP, ADB. Size: M–L (1–2 sessions, spikes first)

- **Scope**:
  - SFTP: report mode from the attrs we already get (it's free); write it with `SETSTAT` permissions, folded like
    `landed_mode` (never wider, no setuid). Birth n/a.
  - S3: write and read rclone's `btime` and `mode` keys beside `mtime` (same format rclone uses).
  - WebDAV: spike `X-OC-CTime` on the Nextcloud fixture; adopt if it sticks, else record n/a.
  - MTP: spike whether Android keeps `DateCreated` from `SendObjectInfo`; adopt or record n/a.
  - ADB: send the folded source mode in `SEND` instead of a fixed `0660` (low value, cheap).
- **Landmines**: every new source report must ride the existing open or listing (rule 6). S3 metadata counts toward the
  2 KB user-metadata limit. A spike that fails is a result: record it in the backend's `DETAILS.md` with an evidence
  anchor.
- **Test plan**: matrix cells per backend on the existing Docker fixtures.
- **DONE**: every matrix cell is Kept or Reported, no cell is a silent drop.

### M8: Close-out. Size: S (1 session)

- An adversarial conformance review against the invariants below, a docs audit reading every commit body, the
  `docs/notes` evidence linked from the right `DETAILS.md`, and this spec wiped per `docs/specs/DETAILS.md`.

## The conformance matrix

One table, one assertion per (source backend, destination backend, metadata class), shaped like the existing
`backend_suites/*_test_support.rs` bodies but data-driven:

- **Cells**: `(source, dest, class) → Expect::{Kept, Reported, NotApplicable}`, with `Expect` DERIVED from the two
  volumes' `MetadataCaps` rather than hand-written, plus a short hand-written exception list (each with a reason) for
  the honest gaps of a phase. A hand-written `Kept` that caps say is impossible fails the build.
- **One body**: `assert_metadata_cell(src: Arc<dyn Volume>, dst: Arc<dyn Volume>, class)` writes a fixture with that
  class set on the source, runs a real copy through the engine, and checks either the value landed (within the
  destination's resolution: FAT 2 s, SMB 100 ns, whole seconds elsewhere) or the operation's loss report names the
  class. That second branch is what keeps the user-facing note honest.
- **Backends**: local (APFS temp dir; HFS+ via `disk_images` in the ignored real-image lane), `InMemoryVolume` (gets a
  reference implementation of every class), and every Docker fixture that exists (SMB, SFTP, WebDAV, S3), plus the
  virtual MTP and ADB doubles where they exist. Each backend's test file stays one line per cell group, like
  `sftp_transfer_integration_test.rs:186-188` today.
- **Per-volume half**: `cmdr_fs::volume::conformance::metadata` (beside `modification_date.rs`) checks one volume's
  write-then-read of each class it claims, so a backend can't claim a cap it doesn't honor.

## Invariants

1. A metadata failure never fails a copy, and is never silent: it's counted and reported (rules 1, § "Telling the
   user").
2. Dates are set last, in one call, on the temp before landing; the landing rename carries them.
3. A staged temp never carries a rename-blocking flag or ACL.
4. Only created items get metadata; a merged-into folder keeps its own.
5. Mode never widens and never escalates on a routed path.
6. No per-file round trip is added to a remote source.
7. Every matrix cell is Kept, Reported, or NotApplicable by caps; none is a silent drop.
8. The temp's NAME and the staging decision are untouched by this work.

## Open questions for David

1. **Finder as the reference**: could you check once, by hand, whether Finder keeps "Date Created" and Locked when
   copying a file and a folder to a USB stick? Our own tests can't drive Finder without a TCC prompt. It only sets
   expectations; the plan keeps birth either way.
2. **Pull the extract quarantine fix forward?** Extracting a quarantined download with Cmdr strips Gatekeeper's mark,
   which `ditto -x` and bsdtar don't. It's small (S) and independent; I'd ship it first.
3. **Compress with `__MACOSX`?** Finder's Compress and `ditto -c -k --sequesterRsrc` store xattrs (tags, resource forks)
   in `__MACOSX/` entries, which Windows users see as clutter. (a) Do it like Finder, (b) never, (c) only when some
   source actually has xattrs worth keeping. I'd pick (c).
4. **Locked files on a same-volume copy probably fail today** (reproduced with the same syscalls, not yet through Cmdr).
   M0 confirms it; if it's real it's a bug fix that could ship ahead of the rest with M1's landing change.
5. **The loss note's wording and placement** are yours to rewrite; the draft above is a placeholder.
