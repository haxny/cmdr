# What opening a real restic repo costs

Evidence for the restic volume's index lifecycle (#422, `docs/specs/restic-volume-plan.md` § "1. Measure on David's NAS
repo"). Measured with `crates/cmdr-restic/examples/open_cost.rs`, which drives `rustic_core` directly through a counting
copy of the crate's storage adapter (every write refused and counted: zero across every run).

## Setup

- **Where**: rustic_core 0.13.0, release build, macOS 27.0.1 on an M3 Max (16 cores), 2026-10-20.
- **Repo**: `/Volumes/naspi/backups/m3-macbook-restic` on the QNAP TS-464 over Tailscale on a direct LAN path (ping RTT
  ~3.6 ms). Nine snapshots, one key file, 45 index files totalling 112,303,608 bytes, 2,382,907 blobs, 131.9 GiB stored
  (261.8 GiB uncompressed). The latest snapshot is ~182 GiB.
- **Two parents**: `mount` is a `LocalPosixVolume` over the OS smbfs mount; `smb` is a direct `cmdr-smb` session (smb2
  0.32.1) to the same share. Connecting the direct session took 0.11–0.12 s.
- **Method**: one phase per process under `/usr/bin/time -l`, so peak RSS is that phase's alone. Every number ran at
  least twice; both are listed. `no_cache` unless the row says otherwise.
- ❗ **The mount's second runs are the macOS smbfs CLIENT cache**, not the NAS's: an index load at 350 MB/s beats the
  wire. Read the mount's first run as cold.

## Numbers

Unlock and snapshot list (no index):

- **Unlock** (`open`: scrypt + one key + config): 0.41–0.61 s across all 20+ runs, either parent. One `keys/` file.
- **`get_all_snapshots`**: nine snapshots, 4,551 bytes. Mount 0.088 s cold, 0.016–0.027 s warm; SMB 0.033–0.050 s.
- **Peak RSS, open + snapshots**: 44–46 MB for the whole process.
- **Listing `index/`**: mount 0.24 s, SMB 0.023–0.026 s.

Index load (both read all 45 index files, 112.3 MB):

- **`to_indexed_ids`**: mount 1.61 s cold, 0.32 s warm; SMB 2.94 s and 2.52 s. Peak RSS 555–620 MB.
- **`to_indexed`**: mount 2.07 s cold, 0.20–0.44 s warm; SMB 4.25 s and 3.97 s. Peak RSS 575–690 MB.
- **Peak concurrent parent reads during the load: 16**, every run, which is rayon's global pool on this 16-core Mac.
  Each one is a pool worker blocked in our adapter for a whole ~2.5 MB index file.
- Peak RSS is ~5x the index files' total size (560 MB / 112 MB), ~240 bytes per blob.

Browsing and reading (latest snapshot):

- **Top three levels** (160 trees, 672 nodes, 160 ranged pack reads, 741 KB): mount 0.61 s cold, 0.03–0.11 s warm; SMB
  2.16–2.30 s, the same on every round. That's ~14 ms per tree on SMB (about four round trips) against ~3.8 ms through
  the mount.
- **Under `to_indexed_ids` there's no blob cache**: walking the same 160 trees again re-reads all 160 ranges.
- **100 MiB of a 1.7 GB already-compressed file** (4 MiB `read_file_at` calls, one pack read per blob, ~1.4 MB each, 129
  MB of packs read for 105 MB of content): mount 34.5 MB/s cold, 143–146 MB/s warm; SMB 14.2, 19.1, 19.3, and 20.3 MB/s.
  Finding the file took a 4–16 s tree search, not part of the read.
- A first try picked a 470 MB sparse disk image: 100 MiB of it was 2.7 MB of packs. Throughput needs an incompressible
  file, which is why the probe looks for one by extension.

rustic's cache (`cache_dir` set, then `to_indexed_ids` + the three-level walk):

- **Cold**: the index load is the same read (mount 1.83 s; SMB 4.86 s and 5.51 s), but the walk fetches **whole tree
  packs** into the cache: 16 reads, 264.8 MB, for the 160 trees the uncached walk got from 741 KB. Walk time mount 4.88
  s, SMB 5.57–5.78 s. Peak RSS 794–825 MB.
- **Warm**: zero repo reads after unlock. Index load 0.25 s, walk 0.009 s, total process 0.72–0.87 s, RSS 539–554 MB.
- **Cache size after one walk**: 360 MB (index 107 MB, tree packs 253 MB, snapshots 36 KB).

Baseline, `restic` 0.19.1 CLI over the mount, `--no-lock --no-cache`:

- **`snapshots`**: 0.85 s and 0.47 s, 63 MB RSS.
- **`stats --mode raw-data`**: 194.8 s and 170.2 s, 2.34–2.36 GB RSS. It walks every tree of all nine snapshots, so it's
  a ceiling, not a peer of our index load.

## Recommendations

- **(a) Load with `to_indexed` up front, and skip `to_indexed_ids`.** Both read the same 112 MB. Full costs ~0.5–1.5 s
  more and ~40–90 MB more RSS here, while upgrading ids to full later is `drop_index` + a second full read (another ~3–4
  s on SMB) plus a fresh unlock. Copy-out and the viewer need full anyway, and full is the state with the blob cache.
- **(b) Hub cap: one indexed repo at a time, and up to eight unlocked-but-unindexed repos.** An index is ~0.5–0.65 GB
  for this 2.4M-blob repo; an unlocked repo with its snapshot list is a few MB. Opening a second index drops the least
  recently used one. Budget by size too: estimate the index's memory as ~5x the `index/` files' total (the listing gives
  it before any load) and show it in the progress, since a 10M-blob repo would want ~2.5 GB.
- **(c) `no_cache`.** The cache makes the first browse slower (the walk reads 265 MB of tree packs and takes 2.5–8x
  longer), raises peak RSS by ~240 MB, and writes 360 MB to disk per repo. Its only win is re-opening (index 0.25 s
  instead of 2–5 s on SMB), and within a session the in-memory index already covers that. If re-open cost bites later,
  cache the `index/` files ourselves in the adapter (112 MB here, no tree-pack download).
- **(d) Drop the index after five idle minutes** (no pane inside the view, no open reader, no running copy), keeping the
  snapshot list and the folder names. Getting it back costs an unlock (~0.5 s, password from Keychain, since rustic
  can't re-open from the master key) plus a load (~2–4 s on SMB, under a second on a warm mount), which the progress
  phase covers. Holding ~0.5 GB idle for longer isn't worth that.
- **(e) Prefetch is fine at this size.** 112 MB of index files held during the load sits on top of a ~560 MB peak that
  rustic reaches anyway, and rustic already keeps 16 in flight. Allow the prefetch below ~256 MB of `index/` and let
  rustic stream above it.

Two findings for the volume milestones:

- **`SmbVolume::read_range` is open + read + close per call** (`crates/cmdr-smb/src/volume/volume_impl.rs`), which is
  the ~14 ms per tree and the serial ~20 MB/s copy-out. The adapter should keep open `open_random_reader` handles per
  pack (a small LRU), and copy-out should read several blobs ahead in parallel.
- **The index load blocks all 16 rayon workers** for 2–5 s on SMB, as the plan's landmine predicted. Search and icons
  share that pool, so the load should run while nothing else needs it, or rustic's fan-out should be fenced.

## Draft #422 comment (not posted)

> Measured the open cost on my NAS repo (9 snapshots, 2.4M blobs, 132 GiB, 112 MB of index files; rustic_core 0.13.0, M3
> Max, NAS over Tailscale on the LAN).
>
> - Unlock + snapshot list: ~0.5 s, ~45 MB RSS. Browsing the dated folders is instant.
> - Index load: 2.5–4.3 s over a direct SMB session, 1.6–2.1 s cold through the mount. Peak RSS ~0.55–0.7 GB (~5x the
>   index files' size).
> - rustic's cache: the first browse downloads whole tree packs (265 MB for 160 trees) and writes 360 MB to disk. The
>   warm re-open is great (0.25 s), but not worth that. Going with `no_cache`.
> - Copy-out over direct SMB is ~20 MB/s today, because every ranged read opens and closes the pack. Keeping pack
>   handles open is on the list.
>
> Plan: `to_indexed` up front, one indexed repo at a time, drop the index after five idle minutes. Details in
> `docs/notes/restic/open-cost.md`.
