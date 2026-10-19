# Verify: `index-scan`

Verifier pass over `index-scan-hunt.md`. Read-only; every claim below was re-traced in the current tree.

### index-scan-1

- **Verdict:** duplicate of index-store-1
- **Severity:** medium (owned by the index-store verifier)
- **Evidence:** `index-store-hunt.md:14` reports the same mechanism at the same sites:
  `AppOperationIndex::recursive_size` (`apps/desktop/src-tauri/src/operations_host/index.rs:30`) drops
  `recursive_size_complete` / `_stale` / `_pending` and hands a bare `u64` to `resolve_conflict`
  (`crates/cmdr-operations/src/conflict.rs:356`), which the conflict dialog renders as an exact size. Not re-verified in
  depth here, per the lead's instruction.

### index-scan-2

- **Verdict:** confirmed
- **Severity:** medium (agree: privacy leak of content from an explicitly excluded folder, no data loss)
- **Evidence:**
  - Symlinks are indexed as non-directory rows: `crates/cmdr-index/src/indexing/scanner/insert_visitor.rs:322-323`
    (`is_dir = file_type == Dir`, `is_symlink = file_type == Symlink`); the bulk reader maps
    `VLNK => RawFileType::Symlink` (`walker/bulk_read.rs:267`), and `read_dir` fallback does the same
    (`walker/mod.rs:338`). The row is pushed with `is_directory: false, is_symlink: true` (`insert_visitor.rs:421-427`).
  - Both media walks ignore `is_symlink`: the whole-index walk selects `WHERE is_directory = 0`
    (`media_index/scheduler/enrich.rs:140`); the scoped walk filters `!c.is_directory` only (`enrich.rs:239`) over
    `list_children_on`, which returns every child (`indexing/store/entries.rs:125-131`). Qualification is by name only
    (`qualify_dir`).
  - The exclusion veto runs on the LINK path, never the resolved target: `process_image` checks
    `is_excluded(&image.path)` before analyze and again before upsert (`media_index/scheduler/pool.rs:273`, `:296`);
    `image.path` is `join_path(dir, name)` of the link row (`enrich.rs:107`, `:244`). `NetworkEnrichConfig::is_excluded`
    is a pure string-prefix test (`media_index/network/config.rs:71-73`, via `path_is_within`). No `canonicalize`,
    `realpath`, `symlink_metadata`, or `O_NOFOLLOW` anywhere under `media_index/` (`rg` returned nothing).
  - The local read follows the link: `ImageInput { bytes: None }` for local (`pool.rs:285-290`) → `decode_thumbnail`
    does `std::fs::read(path)` (`media_index/backend/vision/mod.rs:527`), which follows symlinks.
  - Results ARE surfaced: every `MediaIndex` read filters through `ReadExclusion::hides` (`media_index/read/mod.rs:173`,
    `:182`, `:243`, `:284`, ...), which uses the same path-prefix predicate on the stored (link) path
    (`media_index/read/exclusion.rs:10-11`), so the row passes. The MCP agent tools read through `MediaIndex`:
    `apps/desktop/src-tauri/src/mcp/executor/photos.rs:29` (`search_photos`) and `mcp/executor/image_facts.rs`; the
    exclusion doc itself says that filter is what keeps an excluded folder's OCR text away from a cloud model
    (`read/exclusion.rs:7-9`).
  - Guard search: no test or doc in `media_index/` mentions symlinks; the coverage gate `should_enrich` is also judged
    on the link path, so a link can pull in a target outside the importance/volume scope too.
- **Refutation attempts that failed:** directory symlinks aren't descended (`is_dir` is false for them), but that only
  stops folder-wide leaks, not per-file links; the post-analyze TOCTOU re-check is on the same link path.

### index-scan-3

- **Verdict:** confirmed (the one step the hunter left open is traced)
- **Severity:** low (agree: wedges Cmdr's own image indexing, no user data at risk)
- **Evidence:**
  - FIFOs/sockets/devices DO get rows: the bulk reader maps any other vnode type to `RawFileType::Other`
    (`walker/bulk_read.rs:268`) and returns an entry with `stat: None` rather than dropping it (`bulk_read.rs:285`,
    `:313`; scanner `DETAILS.md` "Degrade, never drop"); the visitor then `symlink_metadata`s it and pushes a row with
    `is_directory = false` (`insert_visitor.rs:353-356`, `:421-427`). So a FIFO named `x.jpg` qualifies in both walks
    (same `is_directory = 0` filters as index-scan-2).
  - The local read is unbounded and blocking on the serialized worker: `std::fs::read(path)`
    (`backend/vision/mod.rs:527`); `open(O_RDONLY)` on a writer-less FIFO blocks indefinitely. No size cap on the local
    path (`MAX_FETCH_BYTES` exists only in `network/fetch.rs`; `rg` finds no cap or timeout in `backend/` or `pool.rs`).
  - `dispatch` waits on `rx.recv()` with no timeout (`backend/vision/mod.rs:141-142`), so the worker holding that job
    never returns and the pass never emits `media-enrich-terminal`. `gate::should_stop` is only checked between images,
    so it can't interrupt a blocked read.
- **Note:** with parallel backends (`pool.rs`, N independent workers) only one worker wedges, but the pass still can't
  complete, so the outcome the hunter describes holds.

### index-scan-4

- **Verdict:** plausible
- **Severity:** low (agree)
- **Evidence:**
  - The drop path is as described: `StreamFlags::from_bits(flags).ok_or(CallbackError::ParseFlags)?` at
    `crates/fsevent-stream/src/stream.rs:396`, `:421`, `:441`, `:465`; `filter_map` logs and drops
    (`stream.rs:332-344`); the overflow flag is set only on `try_send` failure (`stream.rs:346-349`).
  - No CURRENT bit triggers it: `flags.rs:11-36` declares every flag in the macOS 27.0 SDK's `FSEvents.h`
    (`0x1`–`0x400000`, ending at `ItemCloned`; checked against `MacOSX27.0.sdk` headers on 2026-10-19). So it fires only
    if Apple adds a bit or sets an undocumented one. Using `from_bits_retain` is a free hardening.
