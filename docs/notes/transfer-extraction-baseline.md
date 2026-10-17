# Transfer-engine extraction: before and after

The measured before and after for lifting `file_system/write_operations/`, `operation_log/`, and
`file_system/volume/backends/` (`LocalPosixVolume`) out of the app crate into `cmdr-operations`, `cmdr-operation-log`,
`cmdr-local`, and `cmdr-collation` (#415, plan: `docs/specs/transfer-engine-crates-plan.md`). The comparison only means
something if the method matches, so every number carries its command.

**Before**, measured 2026-10-16 on an Apple M3 Max (16 cores, 64 GB), macOS 27.0.1, rustc 1.99.0 (`b940084d7`,
2026-09-28), cargo 1.99.0, 275 GiB free on the data volume. Tree: the `transfer-engine` worktree at commit `4893fa184`,
before any code moved (the commit that adds this note changes docs only, so it's the same code). The app crate was
482,672 lines of Rust. Other Claude sessions shared the Mac, but nothing heavy ran beside the measurements (`ps` showed
OrbStack at half a core to one core and occasional Spotlight indexing); the load before each run is recorded with it.

**After**: not measured yet. The plan pairs every build row by default (§ "Measurement method"): each one is taken
twice, minutes apart on the same toolchain, once on the extracted tree and once on `4893fa184` in a sibling worktree.
Renovate moves deps and the toolchain over the weeks this runs, so the "before" figures below are the context record,
and the paired re-run is the comparison.

## The short version

- **Type-checking the app after an engine edit costs 5.5 s, and building its unit tests costs 15.0–15.4 s**, whichever
  of the three areas the edit lands in. That's the loop the extraction exists to shrink: today there's no way to ask for
  less than the whole 483k-line app crate.
- **An app build after an engine edit takes 10.8 s; after an app edit, 13.2 s.** Expect the first to stay flat or grow
  slightly after the move (an upstream edit still recompiles and relinks the app; the index extraction saw +11%).
- **A clean app-crate build with warm deps takes 42.5 s; a clean release build, 211–212 s** for a 119,734,408-byte
  `Cmdr`.
- **The `desktop-rust-tests` lane runs in about 38 s warm** (11,808 tests), and the engine's own slice (2,143 tests) in
  10.4 s.
- **Criterion reproduced within ±1.2% across two runs on every case.** The committed baseline is what the crate bench
  compares against.

## Read this before comparing

- **Every timed `cargo` command carries `--features cmdr/virtual-mtp`**, the feature set every check lane builds with,
  so a run never flips features and evicts other sessions' warm artifacts. The release row carries it too, which is why
  `Cmdr` is bigger than a shipped build; compare release sizes only against a release built the same way.
- **Thin LTO is on** (`[profile.release] lto = "thin"`), and `bench` inherits `release`, so every Criterion number is a
  thin-LTO number.
- **Criterion runs under mimalloc; the test suite doesn't.** `test_support.rs` swaps in `COUNTING_ALLOCATOR` for test
  builds only (`docs/notes/index-extraction-baseline.md` § "Read this before comparing"). Nothing here is
  allocator-sensitive, but a memory figure would have to compare test allocator against test allocator.
- **Load is the 1-minute `vm.loadavg` just before each run.** It's an average, so a reading right after a heavy build
  (this measurement's own warm-up, or the previous release build) still carries that build: several readings of 5–10
  below were the tail of our own previous run, with `ps` showing nothing else busy.
- **CPU seconds sit beside wall seconds** (user + sys of every child). They survive a loaded machine far better than
  wall time (`docs/notes/archive-extraction-baseline.md` explains why), so if the paired re-run happens under load,
  compare CPU seconds first.
- **Medians, five runs, after one untimed warm-up**, unless a row says otherwise. Every edit is a real string change,
  never a `touch`.

## The driver

`scripts/transfer-baseline/` (a Go program, run with `go -C scripts/transfer-baseline run .`) times one scenario: before
every run (warm-up included) it rewrites a log string to `<anchor> [bench <i>]`, runs an optional untimed pre-step,
records the load, and times the command; it prints median / min / max of wall and CPU seconds, flags any run whose
output shows cargo waiting on the build lock, and restores the file on exit. It runs from the repo root wherever you
call it from.

```sh
B() { go -C "$(git rev-parse --show-toplevel)/scripts/transfer-baseline" run . "$@"; }
COPY=apps/desktop/src-tauri/src/file_system/write_operations/transfer/copy/mod.rs
LOG=apps/desktop/src-tauri/src/operation_log/writer.rs
POSIX=apps/desktop/src-tauri/src/file_system/volume/backends/local_posix.rs
APP=apps/desktop/src-tauri/src/commands/file_system/write_ops.rs
```

Anchors: `"copy_files_with_progress: starting"` in `$COPY`, `"prune failed"` in `$LOG`,
`"doesn't support an atomic no-replace rename"` in `$POSIX`, and `"candidate implies a source"` (an `.expect` message;
the file has no log string) in `$APP`.

## Build times

### Inner loop: an engine edit, then check or build tests (rows 1 and 3)

`B --edit <file> <anchor> -- cargo check -p cmdr --lib --features cmdr/virtual-mtp`, and the same with
`cargo test -p cmdr --lib --no-run --features cmdr/virtual-mtp`. After the move, the command targets the crate being
edited (`-p cmdr-operations`, `-p cmdr-operation-log`, `-p cmdr-local`).

- **Engine (`$COPY`), `cargo check`**: **5.54 s** (5.48–5.61), CPU 5.49 s, load 3.5–4.4
- **Engine (`$COPY`), `cargo test --no-run`**: **15.35 s** (15.06–15.49), CPU 19.60 s, load 2.8–4.7
- **Operation log (`$LOG`), `cargo check`**: **5.44 s** (5.41–5.63), CPU 5.41 s, load 3.1–3.8
- **Operation log (`$LOG`), `cargo test --no-run`**: **15.04 s** (14.97–15.32), CPU 19.42 s, load 3.0–4.4
- **LocalPosix (`$POSIX`), `cargo check`**: **5.48 s** (5.41–5.62), CPU 5.45 s, load 4.0–5.2
- **LocalPosix (`$POSIX`), `cargo test --no-run`**: **15.09 s** (14.95–15.34), CPU 19.46 s, load 3.3–4.4

The three areas cost the same, which is the point: each edit pays for the whole app crate. Every spread is within ±2%,
far tighter than the index note's 23–30 s `--no-run` range, so these need no range treatment.

### Whole-app builds (rows 2, 4, and 5)

- **Engine edit (`$COPY`), then `cargo build -p cmdr --features cmdr/virtual-mtp`**: **10.75 s** (10.63–10.77), CPU
  16.31 s, load 5.7–9.9 (the tail of the warm-up build; the spread stayed at 1.3%)
- **App edit (`$APP`), then the same build**: **13.24 s** (13.16–13.40), CPU 19.67 s, load 3.0–4.5
- **Clean crate build, deps warm**:
  `B -n 3 --no-warmup --pre "cargo clean -p cmdr" -- cargo build -p cmdr --features cmdr/virtual-mtp`: **42.54 s**
  (42.39–44.05), CPU 72.36 s, load 4.0–5.5. After the move, the pre-step also cleans the four new crates
  (`cargo clean -p cmdr -p cmdr-operations -p cmdr-operation-log -p cmdr-local -p cmdr-collation`).

An app edit costs more than an engine edit, as in the index note: `commands/` is `#[tauri::command]` code, so changing
it re-expands the macro surface the IPC builder is generated from.

### Release (row 6)

`B -n 1 --no-warmup --pre "cargo clean --release" -- cargo build --release -p cmdr --features cmdr/virtual-mtp`, twice,
then `stat -f%z target/release/Cmdr`. `-p cmdr`, not the whole workspace, so `index-query` and `operation-log-dump`
(whose dependencies change in the move) stay out of it.

- **Run 1**: **211.28 s**, CPU 2,079 s, load 4.2
- **Run 2**: **211.91 s**, CPU 2,072 s, load 3.8
- **`Cmdr`**: **119,734,408 bytes** both times (deterministic)

## Test lanes (row 7)

**`pnpm check desktop-rust-tests --fresh`**, duration from `~/.local/share/check-runner/cmdr/check-log.csv`. Six runs:
the first rebuilt the lane's test binaries (130.0 s, discarded as the warm-up), then **58.24, 38.32, 37.35, 37.85, and
38.73 s: median 38.3 s**, load 4.9–7.7 (each reading carries the previous run). 11,808 tests run, 810 skipped. The
spread comes from the lane's own retries, not from the build:

- The 58.24 s run retried two tests alone (`volume::delete_test::delete_honors_the_shared_non_recursion_contract`,
  `volume::conformance_test::rename_honors_the_shared_no_clobber_contract`), and the 38.32 s run retried one
  (`backend_suites::adb_index_test::a_cmdr_copy_onto_a_phone_and_a_delete_on_it_patch_its_index`). The lane classified
  every one as "starved under load, passed alone", so the runs still pass.
- The 38.73 s run FAILED on one test that aborted with SIGBUS (signal 10) in 0.037 s:
  `cmdr-index indexing::lifecycle::cover::cold_drive_tests::toggles::a_start_that_meets_a_dead_index_rebuilds_it`. It
  didn't reproduce in 30 runs alone (`cargo nextest run -p cmdr-index --stress-count 30 -E 'test(…)'`). The duration is
  still a full run, so it counts here.

Compare the median, and expect the retries to recur: they're the lane's existing load sensitivity, not the engine's.

**The engine's own slice**,
`B -n 3 -- cargo nextest run -p cmdr --lib --features cmdr/virtual-mtp -E 'test(/file_system::write_operations::|operation_log::|volume::backends::/)'`:
**10.36 s** (10.09–10.84), CPU 57.90 s, load 4.8–7.5; 2,143 tests (nextest's own summary: 9.25 s). The warm-up rebuilt
the lib test binary after the lane runs (86.6 s), so the lane and a plain `cargo nextest` don't share every artifact;
warm up before timing either. After the move:
`cargo nextest run -p cmdr-operations -p cmdr-operation-log -p cmdr-local`.

## Criterion (row 8)

The harness is `apps/desktop/src-tauri/benches/transfer_benchmarks.rs` (the case list and what each one guards are in
the plan's bench milestone and `docs/tooling/testing.md` § "`criterion` (benchmarks)"). Two consecutive runs of

```sh
cargo bench -p cmdr --bench transfer_benchmarks --features cmdr/transfer-bench,cmdr/virtual-mtp -- --save-baseline before
```

with the bench binary built beforehand and the load let down first (2.6 and 4.5 before the runs). Medians, run 1 then
run 2:

- **`local_copy_small_files/1000`**: 226.22, 223.76 ms (−1.1%)
- **`local_copy_small_files_journaled/1000`**: 241.84, 242.00 ms (+0.1%)
- **`stream_large_file/64MiB`**: 45.26, 44.73 ms (−1.2%)
- **`delete_walk/5000`**: 157.46, 155.71 ms (−1.1%)
- **`scan_preview/10000`**: 27.48, 27.37 ms (−0.4%)

**Every case reproduced within ±1.2%.** Criterion called run 2 "no change" on four cases and "within noise threshold" on
`scan_preview`. Journaling costs about 7–8% on the small-file copy (242 against 224–226 ms).

**Compare medians, not means.** The disk-bound cases have a long right tail: the copy cases' means sit 8% above their
medians (244.5 and 241.5 ms against 226 and 224), and `delete_walk`'s run-1 mean was 172 ms against a 157 ms median.
Criterion's printed `time:` line is the mean; the median is in `estimates.json` (`.median.point_estimate`, in ns).

For context, the bench's first two runs, under load 35–50, moved −21%, −12%, −16%, +3%, and +2.6%: these cases need a
quiet machine.

**The committed baseline**: `docs/notes/transfer-extraction-baseline-criterion/` holds run 2's `before/` directory per
case (`estimates.json` plus `sample.json`, `tukey.json`, and `benchmark.json`, which Criterion's `--baseline` comparison
also reads), because `target/` doesn't survive a `cargo clean` or a fresh worktree. To compare, restore it first:

```sh
mkdir -p target/criterion && cp -R docs/notes/transfer-extraction-baseline-criterion/. target/criterion/
cargo bench -p cmdr-operations --bench transfer_benchmarks --features cmdr-operations/testing -- --baseline before
```

## Complexity (row 9)

Run from `apps/desktop/src-tauri/src`. "Production" uses the plan's survey filter, written once:

```sh
PROD=(--glob '!*test*' --glob '!**/tests/**' --glob '!**/copy_tests/**')
ENGINE=(file_system/write_operations operation_log)
```

- **Statics: 53 total, 21 compiled into production.** All: `rg -n '^\s*(pub(\([^)]*\))?\s+)?static\s' "${ENGINE[@]}"`
  (53; the survey counted 52 the same day). Production: top-level ones in production files,
  `rg -n '^(pub(\([^)]*\))?\s+)?static\s' "${ENGINE[@]}" "${PROD[@]}"` (21), minus the one `#[cfg(test)]` among them
  (`scan_bridge.rs:434` `OBSERVED_TICKS`), plus the `CURRENT_TASK_PROBE` task-local (`transfer_probe.rs:859`) = 21. The
  named list is the plan's § "Statics".
- **`crate::` reach-ins (production), counted per mention, doc comments included:**
  - From the engine, not into itself or the operation log: **340**.
    `rg -o --no-filename 'crate::[A-Za-z_]+(::[A-Za-z_]+)*' file_system/write_operations "${PROD[@]}" | rg -v '^crate::file_system::write_operations' | rg -vc '^crate::operation_log'`
  - From the engine into the operation log: **106** (same, ending `| rg -c '^crate::operation_log'`).
  - From the operation log, not into itself: **21** (over `operation_log`, dropping `^crate::operation_log` and
    `^crate::file_system::write_operations`); into the engine: **0**.
  - From `volume/backends/`, not into itself: **17**
    (`rg -o --no-filename 'crate::[A-Za-z_]+(::[A-Za-z_]+)*' file_system/volume/backends "${PROD[@]}" | rg -vc '^crate::file_system::volume::backends'`).
  - The survey's 331 + 96 were counted by hand with a different rule; compare these numbers only with themselves.
- **Non-comment production lines naming `tauri` / `tauri_specta`: 49**, none in `volume/backends/`.
  `rg -n -t rust '\btauri(_specta)?\b' "${ENGINE[@]}" file_system/volume/backends "${PROD[@]}" --glob '!*_bench.rs' | rg -v '^[^:]+:[0-9]+:\s*//' | wc -l`
  (the bench glob drops `copy_bench.rs`'s `src-tauri` path comment).
- **`module-cycles` homes in these areas: 0.**
  `rg -c 'write_operations|operation_log|backends' scripts/check/checks/module-cycles-allowlist.json` (from the repo
  root) finds nothing.
- **`lock-poison` exceptions: 20**, in four files: `delete/walker.rs` 1, `scan_cache.rs` 13, `state.rs` 1,
  `status_cache.rs` 5.
  `rg 'write_operations/|operation_log/|volume/backends/' scripts/check/checks/lock-poison-allowlist.json`.

## Public surface (row 10)

Distinct engine items the app names from outside the engine: **115**. From the repo root:

```sh
rg -o --no-filename 'write_operations::[A-Za-z_]+|operation_log::[A-Za-z_:]+' apps/desktop/src-tauri/src \
  --glob '!**/write_operations/**' --glob '!**/operation_log/**' | sort -u            # 81: 59 + 22
perl -0ne 'while (/pub(?:\(crate\))? use write_operations::(\{[^}]*\}|[A-Za-z_]+)/g) { my $b = $1; $b =~ s/[{}\s]//g;
  print "write_operations::$_\n" for grep { length } split /,/, $b }' apps/desktop/src-tauri/src/file_system/mod.rs   # 42
```

The union of the two lists, deduped, is 115: 81 named directly (59 `write_operations::…`, 22 `operation_log::…`) plus
the `file_system/mod.rs` facade's 42 re-exports, eight of which the first list also names. After the move, run the same
`rg` over `cmdr_operations::` / `cmdr_operation_log::`. `index-crate-isolation` now caps these crates' surfaces
(`scripts/check/checks/index-crate-isolation-ceilings.go`), so the after half quotes those counts beside it.
