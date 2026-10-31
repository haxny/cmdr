# SMB transfer session: side-stat latency and throughput (2026-10-29)

The measurement behind the transfer session (`crates/cmdr-smb/DETAILS.md` § "The transfer session", vdavid/cmdr#133) and
the foreground-yield decision it fed.

## Method

- smb2's `benchmarks/read-ahead/` harness (smb2 0.36.0 at `0e7d26a`), with a `--probe-conn same|separate` flag added for
  this (`PROBE_CONN` in `run.sh`). `same` runs the side `stat` (every 20 ms) on a clone of the transfer's own
  connection, which is what Cmdr did before: the pane's listing and the copy on one session. `separate` runs it on a
  connection and session of its own, which is what the transfer session gives the pane. The flag sits on the smb2 branch
  `bench-separate-probe`, unmerged.
- Samba on Alpine in Docker (OrbStack, M1 Max, container capped at two CPUs), variant `auto` (what Cmdr ships: the
  compound frame when the file fits `quick_read_limit` / `quick_write_limit`, else the adaptive stream). One transfer at
  a time, three runs per cell (five for the loaded ones), medians.
- Links: unshaped loopback ("fast"), and `60@3mbit` (+60 ms, 375 KB/s, netem with a 100,000-packet FIFO queue, "slow").
  "Loaded" adds two background writer connections hammering the share on the fast link.
- One `run.sh` call per cell, for example
  `DIRECTION=up PROBE_CONN=separate VARIANTS=auto SIZES=8388608 LOADS=0 ./run.sh up-slow-separate.csv 3 "60@3mbit" 0`
  (fast: `SIZES=8388608,104857600 ... 3 "0" 0`; loaded: `SIZES=104857600 LOADS=2 ... 5 "0" 2`), then `summarize` /
  `summarize-upload`.

## Results (side `stat` p50 / max ms; throughput MB/s)

Downloads:

- Fast, 8 MiB (one compound frame): same 8 / 8, 517 MB/s; separate 2 / 2, 773 MB/s.
- Fast, 100 MiB (streamed): same 3 / 5, 1,332 MB/s; separate 1 / 2, 1,929 MB/s.
- Loaded, 100 MiB: same 4 / 11, 320 MB/s; separate 3 / 7, 477 MB/s.
- Slow, 8 MiB: same 1,442 / 1,504, 0.4 MB/s; separate 1,442 / 1,505, 0.4 MB/s.

Uploads:

- Fast, 8 MiB (one compound frame): same 3 / 3, 43 MB/s; separate 4 / 4, 308 MB/s.
- Fast, 100 MiB (streamed): same 2 / 7, 253 MB/s; separate 2 / 3, 950 MB/s.
- Loaded, 100 MiB: same 2 / 34, 205 MB/s; separate 2 / 9, 291 MB/s.
- Slow, 8 MiB: same 1,441 / 1,504, 0.3 MB/s; separate 1,441 / 1,507, 0.3 MB/s.

## What it says

- **No throughput regression anywhere.** Every cell is equal or faster with the side `stat` off the transfer's
  connection; the fast-link gains are the probe no longer competing for the one connection's ordering. (The 8 MiB fast
  upload's 43 MB/s "same" median is a noisy cell, three runs; the direction holds across every cell.)
- **On a fast link, a browse during a copy drops to a couple of ms**, and the tail under load drops from 34 to 9 ms
  (uploads) and 11 to 7 ms (downloads). A compound frame no longer holds a `stat` behind it (8 → 2 ms).
- **On the shaped slow link it changes nothing: 1.44 s either way, both directions.** netem's queue is ONE FIFO that
  both connections share, and smb2's adaptive window already keeps about one 512 KiB chunk on the wire there, which is
  1.4 s at 375 KB/s. That's the issue's point 7 ("what it can't fix") measured: the remaining wait is the link's queue,
  not the session's. A real network with per-flow fair queuing (macOS's `fq_codel` on Wi-Fi, most home routers) would
  interleave the two flows and do better; this rig can't show that.
- **What this rig doesn't reproduce**: the field cases. The 23 s wait was a single READ on a shared connection, which
  the adaptive read-ahead had already cut to ~1.5 s, and ERR-CNK7M was a loaded QNAP holding Cmdr's WRITEs 5–12 s in the
  client send queue while ten concurrent copies shared one connection. The bench runs one transfer, and Samba in Docker
  on SSD doesn't serialize the way ksmbd on spinning disks does. A by-hand check on the NAS (copy a big folder, browse
  the same share in the other pane) is the open confirmation.

## The foreground-yield decision

The yield (`crates/cmdr-smb/src/volume/foreground_yield.rs`) parks a transfer between chunks while the user browses the
same share. Its original reason, one shared SMB session, is gone. The slow-link cells are why it stays: with a session
of its own, a `stat` still waited 1.44 s behind the copy's bytes in the link's queue, and only a transfer that stops
feeding the queue takes that away. A disk-bound NAS serving both sessions from the same disks is the same shape one
layer down. So the yield stays on in both directions, with its rationale rewritten to the shared link and the shared
disks.
