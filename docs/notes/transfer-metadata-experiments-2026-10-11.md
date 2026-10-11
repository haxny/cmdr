# Transfer metadata experiments (2026-10-11)

Evidence for `docs/specs/transfer-metadata-preservation.md`: what each macOS copy tool and each Cmdr copy shape keeps,
per filesystem, measured rather than recalled. Environment for every result here: macOS 27.0 (26A428), APFS internal
SSD, plus 64 MB `hdiutil` images of APFS, HFS+, exFAT, and FAT32 (the last two served by FSKit), Homebrew `rsync` 3.5.1.
No Cmdr binary ran: the "Cmdr today" rows replay the exact syscall sequences the code issues (cited below), through the
C helper at the end.

## The fixture

- `f.txt`: birth 2015-03-01, mtime 2020-06-15, atime 2021-01-01, mode `0750`, `hidden` flag, ACL `everyone deny delete`,
  xattrs `user.app`, Finder tags (`_kMDItemUserTags`, binary plist), Finder comment (`kMDItemFinderComment`),
  `com.apple.quarantine`, `com.apple.ResourceFork`.
- `locked.txt`: birth 2015-03-01, mtime 2020-06-15, `uchg` (Finder's "Locked").
- `dir/` (one child): birth 2015-03-01, mtime 2020-06-15, mode `0700`, `hidden`, the same ACL, tags, comment, and
  FinderInfo.
- `link`: a symlink to `f.txt`.

## Results

Copy shapes: `cp -pR`, `ditto`, `rsync -aNHAX --crtimes`, Cmdr's chunked path (data, every xattr, then
`utimes(atime, mtime)`, then `chmod`, as `chunked_copy.rs::copy_metadata` does; ACL step left out of the replay), Cmdr's
folder path (`create_dir_all`, then only mtime, as `copy/scanned_dirs.rs`), `copyfile(COPYFILE_ALL)` with and without
`COPYFILE_CLONE` (Cmdr's same-APFS path is the CLONE one), and the candidate "chunked + one `setattrlist` with birth".

- **Same APFS volume.**
  - Clone (`COPYFILE_ALL|CLONE`) keeps everything: birth, flags, ACL, xattrs.
  - `COPYFILE_ALL` without clone, and `cp -p`: birth reads 2020-06-15. `COPYFILE_STAT` never sets birth.
  - `ditto`: file birth kept (it clones), FOLDER birth lost (2020-06-15).
  - Cmdr chunked replay: birth 2020-06-15, flags lost, xattrs kept.
  - Cmdr folder replay: birth 2020-06-15, mode `0755` (source `0700`), tags, comment, FinderInfo, ACL, and `hidden` all
    lost.
  - Cmdr symlink: a fresh link, its own dates are now.
- **Second APFS volume, and HFS+.** Same as above except the clone can't happen: every tool but `rsync --crtimes` and
  the candidate leaves file birth at 2020-06-15. **`ditto` does not keep birth time across volumes** (plain file and
  folder both read 2020-06-15/2020-06-15).
- **exFAT and FAT32 (FSKit).**
  - No APFS-style quirk: a copy's birth reads "now" (2026-10-11) for `cp -p`, `ditto`, `copyfile`, and the Cmdr replay.
  - Birth IS settable: the candidate and `rsync --crtimes` land 2015-03-01.
  - xattrs land as `._` AppleDouble sidecars. `hidden` and `uchg` map to the FAT attributes and survive `cp -p`.
  - Writing `com.apple.ResourceFork` fails (`EINVAL` with 10 bytes, `ERANGE` with 300) for every tool, and makes
    `copyfile(COPYFILE_ALL)` fail the whole file.
- **The candidate** (data, xattrs, then ONE `setattrlist(ATTR_CMN_CRTIME | ATTR_CMN_MODTIME)`) keeps birth and mtime on
  APFS, HFS+, and exFAT, in either value order (birth after mtime, or before).

## The quirks that shape the design

- **APFS and HFS+ pull birth down to mtime.** Setting mtime earlier than the birth time moves birth to that mtime
  (`touch -m` 2010 on a 2015-born file reads 2010/2010). So a copy that sets only mtime shows `birth == mtime`, which
  looks "preserved" whenever a file was never edited after it was made, and wrong for everything else. Setting birth
  AFTER mtime, or both in one call, is never pulled back. A symlink made now and then `touch -h`'d to 2019 also reads
  birth 2019.
- **A rename keeps everything** set on the inode (birth checked directly; xattrs, ACLs, and flags live on the inode).
- **`uchg` or a `deny delete` ACL on a staged temp blocks its rename into place**: `rename(2)` returns `EPERM` and
  `EACCES`. Reproduced with `copyfile(COPYFILE_ALL|CLONE)` to `.cmdr-tmp-x`, then `rename`, which is Cmdr's same-APFS
  landing (`copy_strategy.rs::copy_file_using` → `overwrite.rs::stage_and_land_file`). `rsync` trips the same way on its
  own `.f.txt.XXXXXX` temp.
- **Writing `com.apple.ResourceFork` bumps mtime** on APFS; `user.*`, quarantine, tags, `chflags`, `chmod`, and ACL
  writes don't. Dates go last.
- **`com.apple.provenance`** shows up on every file a process writes; it isn't something to copy.
- **Archives.** `ditto -c -k --sequesterRsrc` writes xattrs as `__MACOSX/<dir>/._<name>` AppleDouble entries. Both
  `ditto -x -k` and `tar -xf` (bsdtar) restore those xattrs, hide `__MACOSX`, AND copy the archive's
  `com.apple.quarantine` onto every extracted file. A zip carries no birth time (extracted birth reads as mtime).

## Review round 1 re-runs

Same machine and OS, APFS internal SSD unless noted, a fresh C helper (`copyfile`, `setattrlist`, `rename`), run as the
file's owner.

- **`clonefile` copies `uchg` whatever the copyfile flags say.** `COPYFILE_ALL|CLONE` and `COPYFILE_DATA|XATTR|CLONE`
  both land `uchg` on the temp, and both renames fail `EPERM`. Only `COPYFILE_ALL|CLONE` carries a `deny delete` ACL
  (rename `EACCES`); the narrower flags left it off. A `uchg` temp can't be `unlink`ed (`EPERM`).
- **Clone, clear, land, re-apply works**: clone a 2015-born `uchg` file, `chflags nouchg` the temp, rename,
  `chflags uchg`: birth 2015-03-01, mtime 2020-06-15, `uchg` all kept.
- **One `setattrlist(CRTIME|MODTIME)` keeps both, in either relation**: birth 2015 / mtime 2020 and birth 2020 / mtime
  2015 both read back exactly. With `FSOPT_NOFOLLOW` it sets a symlink's own dates.
- **Rename blockers beyond the file's own flags**: a `0500` folder can't move to another parent (`EACCES`); a file can't
  be renamed out of a `uchg` folder (`EPERM`) or out of one with `deny delete_child` (`EACCES`); a `uchg` folder can't
  be renamed (`EPERM`).
- **Order constraints**: `uchg` blocks a later ACL write and a later time-set (`EPERM`); an ACL with `deny writeattr`
  blocks a later time-set and `deny writeextattr` a later xattr write (`EACCES`); `deny writesecurity` doesn't block
  `chflags`. A path xattr write on a `0444` file is `EACCES`. A `deny delete` ACL blocks the owner's own `rm`.
- **exFAT (FSKit, 64 MB `hdiutil` image)**: an xattr and a `setattrlist` birth/mtime on a `.cmdr-tmp-x` survive its
  rename (the `._` sidecar moves with it); writing a child's xattr didn't bump the dated parent folder's mtime.

## The helper

`metacp.c`, built with `/usr/bin/clang -o metacp metacp.c`. Modes: `chunked` (the Cmdr replay), `chunked-crtime-last`,
`copyfile-all`, `copyfile-all-clone`, `copyfile-meta` (`COPYFILE_METADATA` onto an existing file), `setcrtime`, and
`rename`. The one-call variant is a 10-line `setattrlist` with `ATTR_CMN_CRTIME | ATTR_CMN_MODTIME` and a packed pair of
`timespec`s. Re-running it needs only the fixture above; ❌ create FAT or exFAT images in a research script only, never
in a test (`cmdr_fs::testing::disk_images`), and detach each exactly once.

```c
static int set_crtime(const char *p, time_t secs) {
  struct attrlist al = {0}; al.bitmapcount = ATTR_BIT_MAP_COUNT; al.commonattr = ATTR_CMN_CRTIME;
  struct timespec ts = { secs, 0 };
  return setattrlist(p, &al, &ts, sizeof ts, 0);
}
// Cmdr's chunked replay: data, every xattr (listxattr/getxattr/setxattr), utimes(atime, mtime), chmod.
// "chunked-crtime-last" adds set_crtime(dst, src.st_birthtimespec.tv_sec) after utimes.
```
