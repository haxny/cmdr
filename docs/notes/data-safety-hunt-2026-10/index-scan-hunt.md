# Hunt: `index-scan` (drive index scan side + media index + `fsevent-stream`)

Scope: `crates/cmdr-index/` scan half (scanner, watch, reconcile, media_index) plus `crates/fsevent-stream/`. Primary
question from the brief: does any of it write to, touch, or follow out of user files, and does a stale index answer feed
a destructive decision elsewhere.

Headline: nothing in the slice writes to user files. The scanner reads with `getattrlistbulk` (no-follow) and
`read_dir`; the media index reads image bytes and keeps everything in memory (no thumbnail files, media_index Decision
5); the only filesystem writes are under Cmdr's own data dir (CLIP model install, ANN files). The surviving findings are
one index answer that reaches a destructive prompt without its honesty flags, two media-read holes (symlinks and special
files), and one silent event drop in the FSEvents FFI.

### index-scan-1. The "file over folder" overwrite prompt shows a partial index total as the folder's exact size, so a user can replace a big folder believing it's tiny

- **Severity:** medium
- **Confidence:** confirmed for the wrong number reaching the prompt; plausible for the user acting on it (a human
  decision)
- **Where:** `apps/desktop/src-tauri/src/operations_host/index.rs:30`, `AppOperationIndex::recursive_size`;
  `crates/cmdr-operations/src/conflict.rs:344-360` (`resolve_conflict`, Stop arm);
  `crates/cmdr-operations/src/conflict.rs:603-617`, `build_conflict_event`;
  `crates/cmdr-index/src/indexing/read/queries.rs:326`, `get_dir_stats_on_volume`;
  `apps/desktop/src/lib/file-operations/transfer/TransferConflictDialog.svelte:72-113`
- **Excerpt:**

```rust
// apps/desktop/src-tauri/src/operations_host/index.rs:30
    fn recursive_size(&self, path: &Path) -> Option<u64> {
        index()
            .dir_stats(&path.to_string_lossy())
            .ok()
            .flatten()
            .map(|s| s.recursive_size)
    }
```

```rust
// crates/cmdr-index/src/indexing/store/mod.rs:105
    /// Whether `recursive_size` is an exact total (`true`) or a lower bound
    /// (`false`), derived backend-side from the subtree's `min_subtree_epoch`
    /// (`> 0` ⇒ exact). The FE renders an exact size when `true`, a `≥` lower
    /// bound (or `—` when size is 0) when `false`. Raw epochs never cross IPC.
    /// See the "Honest sizes" model in `indexing/DETAILS.md`.
    pub recursive_size_complete: bool,
    /// Whether the (exact) `recursive_size` was computed at an older volume epoch
    /// than the current one (accurate-but-stale). Only meaningful when
    /// `recursive_size_complete` is `true`; drives the muted "stale" treatment.
    pub recursive_size_stale: bool,
```

- **Mechanism:** a local copy/move hits a cross-type clash (incoming file, existing folder of the same name) under the
  Stop policy → `resolve_conflict` calls `lookup_indexed_size(dest_path)` → `AppOperationIndex::recursive_size` →
  `Index::dir_stats` → `get_dir_stats_on_volume`, which returns `Some(DirStats)` for ANY resolved directory, including
  one whose subtree was never fully listed (`recursive_size_complete = false`, a lower bound), one computed at an old
  epoch (`recursive_size_stale`), or one with a big change in flight (`recursive_size_pending`) → the adapter keeps only
  the bare `u64` → `build_conflict_event` puts it in `destination_size` and computes `size_difference` from it → the
  dialog renders it as a plain exact size via `formatByteSize`, and the "(larger)" annotation lands on the NEW side when
  the lower bound undercounts. The prompt's primary button is "Overwrite folder with file".
- **Trigger:** first launch or a just-plugged external drive (scan still running), or a folder under ground the index
  only partially covers, e.g. one whose deep subtree a search walk hasn't reached yet. User copies `Report` (a file)
  into a dir that already has a `Report` folder holding, say, 30 GB of which the index has listed 4 KB. The dialog says
  "Existing: 4 KB" vs "New: 2 MB (larger)", and the user clicks "Overwrite folder with file".
- **Guard ruled out:** `resolution_for_clash` refuses the conditional variants across types (so "Overwrite all smaller"
  can't act on this number), but a plain `Overwrite` answered on this prompt is honored by design, and that's exactly
  the decision the number informs. The volume engine (`transfer/volume/conflict.rs:268`) deliberately sends `None` for a
  folder destination; only the local engine sends the index number. The listing UI does honor the flags (`≥` rendering);
  this IPC path drops them. No test pins the prompt's size against an incomplete subtree.
- **Impact:** the user replaces a folder whose real size they were misinformed about. The act itself had consent, so
  this is not silent; recoverability depends on the safe-overwrite path and the op log's rollback (not traced here).
- **Suggested fix:** have `recursive_size` return `None` unless
  `recursive_size_complete && !recursive_size_stale && !recursive_size_pending` (the dialog already renders `None`
  honestly as "(unknown)"), or carry the flags through `WriteConflictEvent` so the dialog can render `≥`.

### index-scan-2. Media enrichment follows symlinks named like images, so a privacy-excluded folder's OCR text lands in search and the agent tools under the link's path

- **Severity:** medium
- **Confidence:** confirmed
- **Where:** `crates/cmdr-index/src/media_index/scheduler/enrich.rs:132`, `for_each_qualifying_image`;
  `crates/cmdr-index/src/media_index/scheduler/enrich.rs:226`, `walk_image_entries_in_dirs`;
  `crates/cmdr-index/src/media_index/backend/vision/mod.rs:522`, `decode_thumbnail`;
  `crates/cmdr-index/src/media_index/network/config.rs:71`, `is_excluded`;
  `crates/cmdr-index/src/indexing/scanner/insert_visitor.rs:322-323`
- **Excerpt:**

```rust
// crates/cmdr-index/src/media_index/scheduler/enrich.rs:136
    let mut dirs = DirTree::load(conn)?;

    let mut stmt = conn
        .prepare_cached(
            "SELECT parent_id, name, modified_at, logical_size FROM entries WHERE is_directory = 0 ORDER BY parent_id",
        )
        .map_err(|e| e.to_string())?;
```

```rust
// crates/cmdr-index/src/media_index/backend/vision/mod.rs:522
fn decode_thumbnail(path: &str, prefetched: Option<&[u8]>) -> Result<CFRetained<CGImage>, VisionError> {
    let owned;
    let bytes: &[u8] = match prefetched {
        Some(b) => b,
        None => {
            owned = std::fs::read(path).map_err(|e| {
```

- **Mechanism:** the scanner records a symlink as a non-directory row (`is_dir = file_type == Dir`, so a `Symlink` is
  `is_directory = 0`, `is_symlink = 1`) → both media walks select `is_directory = 0` and never look at `is_symlink` →
  `qualify_dir` qualifies by name, so `scan.jpg` qualifies whatever it points at → the privacy veto
  `is_excluded(image.path)` tests the LINK's path against the excluded folders, not the target's → the local backend
  reads with `std::fs::read(path)`, which follows the link → OCR text, tags, and the CLIP embedding of the target are
  stored under the link's path → `ReadExclusion::hides` also judges by the stored (link) path, so `search_photos` /
  `image_facts` (the agent tools, `read/exclusion.rs:7-9`) serve the excluded content.
- **Trigger:** a user excludes `~/Documents/IDs` from image indexing; elsewhere in a covered folder there's a symlink
  into it (a "Desktop shortcut" made with `ln -s`, a dotfiles/sync tool's links, a project folder linking an asset
  folder). Directory symlinks aren't descended, but a file symlink is read.
- **Guard ruled out:** the exclusion veto is checked twice (before analyze and before upsert), but both on the link
  path. The scanner's no-follow `getattrlistbulk` keeps the INDEX honest, but the media layer re-opens by path with a
  following read. No test or doc in `media_index/` mentions symlinks. The target also escapes the coverage scope
  (importance threshold, volume scope): a link on the boot disk to `/Volumes/X/…` reads another volume's file.
- **Impact:** privacy, not loss: content from a folder the user explicitly excluded (media_index Decision 10 calls OCR
  of a passport scan "sensitive content") is stored in `media.db` and returned by search and the agent tools. Fixed by
  deleting the rows once the walk skips symlinks.
- **Suggested fix:** select `is_symlink = 0` in both walks (and `list_children_on` filtering in the scoped one), or open
  with `O_NOFOLLOW` in the local read path; either way the GC then collects the existing rows.

### index-scan-3. A FIFO (or a link to a device) named like an image hangs the Vision worker forever, wedging image indexing for every volume

- **Severity:** low
- **Confidence:** plausible (the hang is traced; that the scan admits FIFOs as `Other` rows that reach the walk is
  traced at `walker/mod.rs:340`, the bulk path's type mapping for `VFIFO` not checked)
- **Where:** `crates/cmdr-index/src/media_index/backend/vision/mod.rs:527`, `decode_thumbnail`;
  `crates/cmdr-index/src/media_index/backend/vision/mod.rs:160`, `dispatch`;
  `crates/cmdr-index/src/indexing/scanner/walker/mod.rs:336-343`
- **Excerpt:**

```rust
// crates/cmdr-index/src/media_index/backend/vision/mod.rs:144
    fn dispatch<T>(&self, path: &str, bytes: Option<Vec<u8>>, make_reply: impl FnOnce(mpsc::Sender<T>) -> JobReply) -> T
    where
        T: FromWorkerGone,
    {
        let (tx, rx) = mpsc::channel();
        if self
            .sender
            .send(Job {
                path: path.to_string(),
                bytes,
                reply: make_reply(tx),
            })
            .is_err()
        {
            return T::worker_gone("vision worker thread is gone");
        }
        rx.recv()
            .unwrap_or_else(|_| T::worker_gone("vision worker dropped the job"))
    }
```

- **Mechanism:** the walks select every `is_directory = 0` row, which includes `Other` (FIFO, socket, device) and
  symlinks → for a local volume the bytes are read ON the serialized worker thread with an unbounded
  `std::fs::read(path)` → a FIFO with no writer blocks `open` forever; a link to `/dev/zero` or a huge non-image file
  reads without any cap (the network path caps at `MAX_FETCH_BYTES`, the local path doesn't, and a symlink row has
  `logical_size = None` so no size gate could catch it) → `dispatch` waits on `rx.recv()` with no timeout, so the pass
  never ends and `media-enrich-terminal` never fires.
- **Trigger:** rare: a named pipe or a link to a device whose name ends in an image extension, inside a covered folder.
- **Guard ruled out:** `backend/CLAUDE.md` promises "Hostile input fails CLOSED ... never a panic or a hang"; the
  network fetcher has a timeout and a byte cap, the local read has neither. The memory watchdog would stop a `/dev/zero`
  read eventually, not a FIFO block.
- **Impact:** Cmdr's own state only: image indexing wedges until restart (and re-wedges on the next pass), or the
  process balloons. No user data lost.
- **Suggested fix:** in the local read, `symlink_metadata` first and refuse anything but a regular file, then read
  through the same `take(MAX_FETCH_BYTES + 1)` cap the network path uses.

### index-scan-4. An FSEvents event carrying a flag bit the crate doesn't know is silently dropped, without setting the overflow flag

- **Severity:** low
- **Confidence:** plausible (depends on a future or undocumented flag bit appearing; no current one known)
- **Where:** `crates/fsevent-stream/src/stream.rs:332-344` (callback filter) and `:396` / `:421` / `:441` / `:465`
  (`StreamFlags::from_bits`)
- **Excerpt:**

```rust
// crates/fsevent-stream/src/stream.rs:332
                let events = event_iter(num_events, event_paths, event_flags, event_ids)
                    .filter_map(|event| {
                        if let Err(e) = &event {
                            match e {
                                CallbackError::ToI64 => {
                                    error!("Unable to convert inode field to i64")
                                }
                                CallbackError::ParseFlags => error!("Unable to parse flags"),
                            }
                        }
                        event.ok()
                    })
                    .collect();
```

- **Mechanism:** `StreamFlags::from_bits(flags)` returns `None` when ANY bit outside the known set is present → the
  event becomes `Err(ParseFlags)` → `filter_map` drops it with a log line → the overflow flag (the only "history is
  incomplete, rescan" signal the live loop reads) is never set → the index misses that create/remove/rename until a
  later verification or full scan; replay won't recover it either, because later events advance the stored event id past
  it.
- **Trigger:** a macOS release adding an event flag bit (Apple has added bits before: `ItemCloned`,
  `ItemIsLastHardlink`), or an undocumented bit set on some event today.
- **Guard ruled out:** `parse_fsevent` (`watch/watcher.rs:586`) only reads known bits, so `from_bits_truncate` /
  `from_bits_retain` would lose nothing. The user/kernel-dropped cases are fine: FSEvents also sets `MustScanSubDirs`,
  which is honored.
- **Impact:** index drift (wrong sizes, a missing search hit), recoverable by rescan; Cmdr's own data.
- **Suggested fix:** use `StreamFlags::from_bits_retain` (keep `raw_flags` as today); if a parse failure must stay
  possible, set the overflow flag on it instead of dropping silently.

## Near misses

- CLIP model unzip (`clip/install.rs:310`): zip-slip guarded by `enclosed_name`, and the SHA-256 is verified BEFORE
  unpacking against a pinned hash, all inside Cmdr's data dir. No user-file reach.
- `remove_dir_all` sites (`clip/install.rs:195,378,391`, `scheduler/mod.rs:439`, `ann/mod.rs`): all under
  `clip_model_dir(data_dir)` or the media DB dir, from fixed relative names.
- Scanner writing to user files: none. `getattrlistbulk` without follow (`walker/bulk_read.rs:54`), `read_dir` fallback,
  `symlink_metadata` probes, one xattr READ for File Provider roots.
- Stale index driving journal leaves (`operations_host/index.rs:42`, `search_leaves`): gated on `is_fresh` + subtree
  epoch, and the leaves are `search_only`; rollback uses the top-level unit, so a miss affects search only.
- `expected_totals`: feeds progress bars only (`scan.rs:285`, `scan_preview/local.rs:76`).
- "Overwrite all smaller"/"older" against index sizes: the local engine reduces them from `fs::Metadata` of files
  (`conflict.rs:475`), and cross-type clashes are refused (`resolution_for_clash`), so the index number never decides an
  automatic overwrite.
- Watcher overflow/drops: the FFI channel's `try_send` failure sets the overflow flag, which the live loop and replay
  read (`event_loop/live.rs:355`); user/kernel drops carry `MustScanSubDirs`.
- Network media fetch (`network/fetch.rs`): bounded size, timeout, typed classification; read-only through the Volume
  trait (`open_read_stream_for_scan`).
- Media GC widening: `WalkedDirs` is mint-only-by-walk, so a scoped tick can't GC dirs it didn't walk.

## Covered

Read in full (non-test parts):

- `crates/fsevent-stream/src/stream.rs`, `crates/fsevent-stream/src/flags.rs`
- `crates/cmdr-index/src/media_index/clip/install.rs` (through line 410, the non-test code)
- `crates/cmdr-index/src/media_index/network/fetch.rs` (non-test code)
- `crates/cmdr-index/src/media_index/scheduler/enrich.rs` (lines 1–470)
- `apps/desktop/src-tauri/src/operations_host/index.rs` (non-test code)
- `crates/cmdr-operations/src/journal_search.rs` (header and enumeration)

Read in part (the sites cited above): `indexing/watch/watcher.rs` (macOS `start`, `parse_fsevent`),
`media_index/backend/vision/mod.rs` (dispatch, worker loop, decode), `media_index/network/config.rs` (exclusion
matcher), `media_index/read/exclusion.rs` (header), `indexing/scanner/exclusions.rs` (scope and probes),
`indexing/scanner/insert_visitor.rs` (type mapping), `indexing/scanner/walker/mod.rs` (`read_dir` typing),
`indexing/metadata.rs`, `indexing/read/queries.rs` (`get_dir_stats*`), `indexing/store/mod.rs` (`DirStats`),
`crates/cmdr-operations/src/conflict.rs` (resolution and event build), `durability.rs`, `transfer/volume/conflict.rs`,
`TransferConflictDialog.svelte`. Plus every slice `CLAUDE.md` listed in the prompt.

## Not covered

The slice is roughly 40,000 lines of non-test Rust; I targeted the user-file-reach and index-to-destructive-decision
paths rather than reading it all. Not read: `indexing/reconcile/**` (reconciler, rescan, local_reconcile, verifier),
`indexing/watch/event_loop/**` (live, replay, rename_detect, storm, verification), `indexing/watch/branches.rs`,
`indexing/scanner/mod.rs` and `walker/engine.rs`, `indexing/network_scanner/**`, `indexing/transports/**`,
`media_index/scheduler/` beyond `enrich.rs` (pool, live, lifecycle, reclaim, purge), `media_index/store`, `ann`,
`vector`, `coverage`, and `fsevent-stream/src/{ffi,observer,lib}.rs`. These write only Cmdr's own databases, so a bug
there caps at medium; a follow-up hunt could target rename-by-inode correctness and media GC on a partially listed
index.
