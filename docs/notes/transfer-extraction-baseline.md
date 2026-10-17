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

**After**, measured 2026-10-17 on the same Mac and toolchain (rustc 1.99.0, `b940084d7`), paired: every row ran on the
extracted tree and, minutes apart, on `4893fa184` in a temporary sibling worktree, so the comparison is the paired
re-run, and the "before" figures below are the context record. § "After" has the numbers and their method.

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
the bench's module doc, now `crates/cmdr-operations/benches/transfer_benchmarks.rs`, and `docs/tooling/testing.md` §
"`criterion` (benchmarks)"). Two consecutive runs of

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

Run from `apps/desktop/src-tauri/src`. "Production" uses this survey filter, written once:

```sh
PROD=(--glob '!*test*' --glob '!**/tests/**' --glob '!**/copy_tests/**')
ENGINE=(file_system/write_operations operation_log)
```

- **Statics: 53 total, 21 compiled into production.** All: `rg -n '^\s*(pub(\([^)]*\))?\s+)?static\s' "${ENGINE[@]}"`
  (53; the survey counted 52 the same day). Production: top-level ones in production files,
  `rg -n '^(pub(\([^)]*\))?\s+)?static\s' "${ENGINE[@]}" "${PROD[@]}"` (21), minus the one `#[cfg(test)]` among them
  (`scan_bridge.rs:434` `OBSERVED_TICKS`), plus the `CURRENT_TASK_PROBE` task-local (`transfer_probe.rs:859`) = 21. By
  name: `WRITE_OPERATION_STATE`, `MANAGER`, `OPERATION_STATUS_CACHE`, `LAST_EMITTED_BUSY`, `PREVIEWS`,
  `PREVIEW_SETTLED`, `PROCESS_LEDGER`, `OUTAGES`, `NEXT_OUTAGE_ID`, `REGISTRY` (the probes), `OPERATIONS_APP`,
  `BUSY_APP`, `APP_HANDLE`, the operation log's `JOURNAL`, `ENABLED`, `UNRECORDED`, and `UNRECORDED_LEN`, the overwrite
  aside park's `ARMED`, `IS_PARKED`, and `RELEASED`, and `CURRENT_TASK_PROBE`.
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

## After

Measured 2026-10-17, 14:15–16:20, on the `transfer-engine` worktree at `d6097d29c` (most inner-loop rows at `e96bee4b4`;
the commit between them changes docs only), paired with `4893fa184` in a worktree made for it
(`new-worktree.sh baseline-415 4893fa184`, removed afterwards). The app crate is now 327,554 lines of Rust and the four
crates 166,730 (`cmdr-operations` 153,670, `cmdr-operation-log` 7,768, `cmdr-local` 5,124, `cmdr-collation` 168), each
counted with `rg --files -t rust <dir> | xargs cat | wc -l`.

**The machine was busy.** Load sat at 3–8 for the inner-loop rows and 10–20 for the test lanes; IntelliJ took up to 3.6
cores at times, and the release builds pushed the 1-minute load past 50 for the run that followed them. Every row is
paired, so a busy minute hits both sides, but a single pair can still straddle a spike: each reading below carries its
load, and the rows where the pair disagreed with itself say so.

### The short version (after)

- **The inner loop the extraction exists for got 2.4–18× cheaper.** After an engine edit, `cargo check` on the crate
  takes 1.1 s against 5.6 s for the app, and building its tests 6.8 s against 16.2 s. The operation log and
  `LocalPosixVolume` fell further, to 0.3–0.4 s (check) and 1.0 s (tests) against 5.5–5.8 s and 15.8–16.6 s.
- **An app build after an engine edit costs 7% more (12.8 s against 12.0 s)**, as the plan predicted: cargo still
  rebuilds and relinks the app after any upstream change. An app edit got 9% cheaper (12.7 s against 13.9 s).
- **Clean and release builds are flat**: 44.4 s against 46.0 s for the clean crate build, 219–225 s against 223 s for a
  clean release. `Cmdr` grew 1,416,032 bytes (1.2%), to 121,150,408.
- **The `desktop-rust-tests` lane read 48.0 s against 37.5 s on this worktree, and the cause is this worktree's
  `target/`, not the code.** On an equally fresh `target/`, the same tests run in the same time (§ "Test lanes, after").
- **Criterion shows no change** once the bench's feature drift is neutralized: two paired runs, every case within the
  day's own run-to-run spread. The drift was real (as built then, the bench ran `stream_large_file` 17% faster than
  production behavior would), and the bench now builds without it.
- **Complexity**: production statics 21 → 1, app reach-ins 340 → 0, non-comment `tauri` lines 49 → 0, `lock-poison`
  exceptions 20 → 0, app-named engine items 115 → 55.

### Read this before comparing (after)

- **The crate-scoped commands carry no `--features cmdr/virtual-mtp`**: cargo refuses a feature of a package that isn't
  selected ("none of the selected packages contains this feature"). They compile `cmdr-fs` and the other deps with the
  crate's own feature set, which costs one extra warm-up per command and nothing per run.
- **The paired "before" ran in a fresh worktree** with a cold `target/` (`new-worktree.sh` skips the `target/` clone
  when the main clone sits on another commit), warmed with one untimed `cargo build` and `cargo test --no-run` first.
- **The engine-check row ran twice per side.** The first `4893fa184` set (7.68 s, then 7.56 s, at load 3.7–12.3) came
  right after the fresh tree's first build; a third set taken 12 minutes later read 5.64 s, matching yesterday's 5.54 s
  and the other two areas. The row below uses the third set, and the second `cmdr-operations` set beside it (1.12 s; the
  first read 1.53 s).
- **CPU seconds first, as the before half says**, wherever a pair's loads differ.

### Build times, after

Driver as in § "The driver", with `COPY=crates/cmdr-operations/src/transfer/copy/mod.rs`,
`LOG=crates/cmdr-operation-log/src/writer.rs`, `POSIX=crates/cmdr-local/src/local_posix.rs`, the same anchors, and
`$APP` unchanged.

Inner loop (rows 1 and 3), `B --edit <file> <anchor> -- cargo check -p <crate>` and
`cargo test -p <crate> --lib --no-run`, against the before commands on `4893fa184`:

- **Engine, `cargo check`**: **1.12 s** (1.11–1.13), CPU 1.10 s, load 4.0–4.2, against **5.64 s** (5.58–5.70), CPU 5.61
  s, load 4.2–4.6: **−80%**
- **Engine, `cargo test --no-run`**: **6.77 s** (6.68–6.85), CPU 9.33 s, load 3.7–4.5, against **16.20 s**
  (16.00–16.63), CPU 20.00 s, load 4.8–5.8: **−58%**
- **Operation log, `cargo check`**: **0.36 s** (0.36–0.47), CPU 0.35 s, load 3.5–3.6, against **5.83 s** (5.79–5.91),
  CPU 5.81 s, load 2.9–3.5: **−94%**
- **Operation log, `cargo test --no-run`**: **1.00 s** (1.00–1.01), CPU 1.99 s, load 3.3–3.4, against **16.63 s**
  (16.48–16.86), CPU 20.66 s, load 3.6–5.6: **−94%**
- **LocalPosix, `cargo check`**: **0.31 s** (0.30–0.44), CPU 0.30 s, load 3.3, against **5.49 s** (5.39–5.59), CPU 5.44
  s, load 4.5–5.4: **−94%**
- **LocalPosix, `cargo test --no-run`**: **1.00 s** (0.99–1.03), CPU 1.93 s, load 3.3, against **15.78 s**
  (15.02–16.01), CPU 19.94 s, load 3.7–4.7: **−94%**

Whole-app builds (rows 2, 4, and 5), the commands unchanged except the clean step, which also cleans the four crates:

- **Engine edit, then `cargo build -p cmdr --features cmdr/virtual-mtp`**: **12.79 s** (12.76–13.22), CPU 18.79 s, load
  4.3–5.4, against **11.99 s** (11.55–12.05), CPU 16.94 s, load 5.6–6.8: **+7% wall, +11% CPU**. An engine edit still
  recompiles the crate and then the app that depends on it, now as two units; the index extraction saw the same +11%.
- **App edit (`$APP`), then the same build**: **12.65 s** (11.83–13.10), CPU 18.72 s, load 4.8–7.1, against **13.89 s**
  (13.63–14.23), CPU 20.19 s, load 5.1–7.5: **−9% wall, −7% CPU**, with about a third less app crate to recompile.
- **Clean crate build, deps warm**, `-n 3 --no-warmup`: **44.37 s** (44.15–45.56), CPU 77.81 s, load 6.8–8.6, against
  **45.95 s** (45.93–47.82), CPU 76.12 s, load 6.3–6.8: **−3% wall, +2% CPU**, flat.

Release (row 6), the same command, alternated after → before → before → after:

- **After**: **219.21 s** (CPU 2,170 s, load 4.4) and **225.38 s** (CPU 2,140 s, load 6.2)
- **Before**: 235.16 s (CPU 2,142 s, load 54.9, the previous release build's tail: discard the wall time) and **222.89
  s** (CPU 2,107 s, load 4.0)
- **Flat**: −2% to +1% wall, +1.5% to +3% CPU.
- **`Cmdr`: 121,150,408 bytes** both times, against **119,734,376** for `4893fa184` both times: **+1,416,032 bytes
  (+1.2%)**. Not traced to a cause. (Yesterday's `4893fa184` build was 32 bytes larger; it was built under another
  worktree path, which the binary embeds.)

### Test lanes, after

**`pnpm check desktop-rust-tests --fresh`**, three runs per side after one warm-up each, alternated, durations from
`check-log.csv`:

- **This worktree**: **47.75, 48.00, 48.61 s: median 48.0 s**, load 10.8–18.5; 11,909 tests (101 more than before).
  **`4893fa184`**: **37.49, 37.48, 38.07 s: median 37.5 s**, load 12.9–16.1; 11,808 tests. Two runs on this side and one
  on the other retried a cell the lane then called "starved under load, passed alone" (the `adb_index_test` copy cell
  each time on this side); every run passed, and the 48.0 s median includes one run with no retry (47.75 s).
- **One more pair under `/usr/bin/time -p`**: 52.8 s, user 158 s + sys 186 s, against 40.5 s, user 165 s + sys 137 s.
  The same user time and 49 s more kernel time point at the machine, not the code.

The kernel time comes from this worktree's `target/debug/deps`, which holds 82,623 entries against the fresh tree's
21,591. A `cargo nextest run -p cmdr -p cmdr-operations -p cmdr-operation-log -p cmdr-local --features cmdr/virtual-mtp`
(the same tests the before tree's `-p cmdr` runs, plus 55) took 33.5 and 38.5 s here against 27.2 and 27.5 s on
`4893fa184`, and its slowest tests were every `delete::trash` cell, 2.9–3.0 s each where the before tree ran them in
0.3–0.6 s. That's the shape of #357, where the first NSURL call in a process `readdir`s the test binary's directory,
here through the trash's `NSFileManager`. In a second fresh worktree at `d6097d29c` (13,521 entries), the same command
took **26.9 and 27.2 s against 27.7 s**, and the trash cells dropped out of the lane's slow-test log.

**The lane itself on two fresh trees is unresolved**: 46.9, 61.9, and 46.0 s against 71.9, 64.3, and 43.3 s, at load
11.5–20, which is noise. Expect parity from the nextest result above, re-measure on a quiet machine before quoting a
lane number, and treat a long-lived worktree's `target/debug/deps` as a cost on every test that reaches `NSFileManager`.

**The engine's own slice** (`-n 3`): `cargo nextest run -p cmdr-operations -p cmdr-operation-log -p cmdr-local`, **10.84
s** (9.87–10.94), CPU 47.49 s, load 9.6–9.9; 2,117 tests across three binaries (nextest's own summary 8.95 s). Against
the before slice, **10.24 s** (10.10–10.75), CPU 58.55 s, load 6.0–11.6, 2,142 tests: **+6% wall, −19% CPU**. The sets
differ on purpose: the backend suites (`operations_host/backend_suites/`) still live in the app and build it, so they
left the slice. And the before slice drops one cell, `- test(/adb_index_test::a_cmdr_copy_onto/)`, which failed on
`4893fa184` in both warm-ups at load 3.7 and 10 (a `uploaded.bin.cmdr-tmp-…` staging row left behind); the lane calls
the same cell starved and passes it alone.

### Criterion, after

**The feature drift, neutralized at the "after" side.** The crate bench builds with `cmdr-local/testing` on (dev
dependencies). When these runs were taken, that flipped two behaviors the bench reaches: space answered from `statvfs`
instead of NSURL (`space.rs`), and the 1 ms listing-progress sampler (`local_posix.rs`). The third `testing` behavior
gate in the graph, `cmdr-fs`'s thread-QoS no-op, never runs here: the engine doesn't call it. For the measured runs,
both `cmdr-local` gates were set back to `cfg!(test)` in an uncommitted edit, reverted right after, so the bench ran
production behavior like `4893fa184`'s, whose gates are `cfg(test)` only. Since the final pass, both gates read
`cmdr-local`'s `TEST_FAST_PATHS`, which also requires `debug_assertions`, so the `bench` profile runs production
behavior as committed and a re-run needs no edit (`crates/cmdr-local/DETAILS.md` § "A consumer's test build is a test
build here too").

Medians from `estimates.json`, two pairs, the second in reverse order, after → before:

- **`local_copy_small_files/1000`**: 232.84 vs 226.49 ms (+2.8%); 239.23 vs 241.15 ms (−0.8%)
- **`local_copy_small_files_journaled/1000`**: 244.49 vs 243.09 ms (+0.6%); 261.28 vs 260.21 ms (+0.4%)
- **`stream_large_file/64MiB`**: 50.14 vs 46.62 ms (+7.5%); 46.46 vs 46.52 ms (−0.1%)
- **`delete_walk/5000`**: 167.99 vs 164.59 ms (+2.1%); 171.28 vs 173.73 ms (−1.4%)
- **`scan_preview/10000`**: 32.87 vs 31.48 ms (+4.4%); 33.70 vs 33.56 ms (+0.4%)

Loads: the first pair 4.7–5.9 (the "before" run ended at 19.5), the second 6.3–8.1. **No change**: the two pairs' means
sit at +0.4% to +3.7%, and `4893fa184` alone moved +5% to +7% between its two runs on four of the five cases.

**Against the committed baseline, the comparison measures the day.** The first gates-off run reads +4%, +1%, +12%, +8%,
and +20% against `transfer-extraction-baseline-criterion/`, but `4893fa184` itself read +1%, +0.5%, +4%, +6%, and +15%
against it on the same afternoon. Compare paired runs, or the committed baseline only on a quiet machine.

**What the drift is worth**, one as-built run (gates on, load 6.9–12.1): 232.11, 243.30, **38.72**, 167.72, and 33.90
ms. `stream_large_file` runs 17% faster with the `testing` gates than without (38.7 against 46.5–50.1 ms); the other
four sit inside the gates-off spread. So before the fix, `cargo bench -p cmdr-operations … -- --baseline before` as
written above flattered that one case.

### Complexity, after

Run from the repo root. `PROD` as in the before half;
`CRATES=(crates/cmdr-operations/src crates/cmdr-operation-log/src)` stands in for `ENGINE`, and `crates/cmdr-local/src`
for `volume/backends`.

- **Statics: 22 total (53), 1 in production (21).** All: `rg -n '^\s*(pub(\([^)]*\))?\s+)?static\s' "${CRATES[@]}"`.
  Production, the same rule as before: the top-level search over `"${CRATES[@]}" "${PROD[@]}"` finds one,
  `scan_bridge.rs:441` `OBSERVED_TICKS`, which is `#[cfg(test)]`; add the `CURRENT_TASK_PROBE` task-local
  (`transfer/transfer_probe.rs:861`) = 1.
- **Reach-ins into the app: 0 (340 from the engine, 21 from the operation log, 17 from `volume/backends/`).** The crates
  can't name the app: `rg -c 'cmdr_lib|\bcmdr::'` over the four crates finds nothing.
  - What replaced them, as cross-crate mentions in production, doc comments included:
    `rg -o --no-filename 'cmdr_[a-z_]+::' crates/cmdr-operations/src "${PROD[@]}" | sort | uniq -c`. From the engine:
    `cmdr_fs` 365, `cmdr_operation_log` **106** (106 before, as `crate::operation_log`), `cmdr_archive` 24, `cmdr_local`
    15, `cmdr_collation` 4. From the operation log: `cmdr_fs` 12, and 5 doc comments naming `cmdr_operations` (0 code:
    the crate doesn't depend on it). From `cmdr-local`: `cmdr_fs` 53.
- **Non-comment production lines naming `tauri` / `tauri_specta`: 0 (49).** The before command over the four crates
  counts 1, `compress_estimate.rs:392`, a `src-tauri` path string inside its test module; no crate manifest names a
  `tauri` dependency, and `index-crate-isolation` keeps it that way.
- **`module-cycles` homes: 0 (0).**
  `rg -c 'write_operations|operation_log|backends|cmdr-operations|cmdr-operation-log|cmdr-local' scripts/check/checks/module-cycles-allowlist.json`
  finds nothing.
- **`lock-poison` exceptions: 0 (20).** No entry in `lock-poison-allowlist.json` names the four crates or the old paths.

### Public surface, after

**App-named engine items: 55 (115).** The before command re-pointed:
`rg -o --no-filename 'cmdr_operations::[A-Za-z_]+' apps/desktop/src-tauri/src | sort -u` (33) plus
`rg -o --no-filename 'cmdr_operation_log::[A-Za-z_:]+' apps/desktop/src-tauri/src | sort -u` (22). There's no facade any
more (no `pub use cmdr_operations` in the app). Brace-group imports (`use cmdr_operations::{…}`) escape this regex both
times, so it's a like-for-like count, not a census.

**The enforced ceilings** (`index-crate-isolation-ceilings.go`, set to exactly what each crate exposes):

- **`cmdr-operations`**: 82 root promises, 56 handle methods on `Operations`, 1 public module (`host`), 25 subsystem
  items
- **`cmdr-operation-log`**: 11 root promises, 6 handle methods on `OperationLog`, 7 public modules, 75 subsystem items
- **`cmdr-local`**: 12 root promises, 9 public modules, 39 subsystem items
- **`cmdr-collation`**: 2 root promises, no public modules, 4 subsystem items
