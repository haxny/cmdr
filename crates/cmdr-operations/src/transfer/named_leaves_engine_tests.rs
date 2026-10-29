//! Per-source names on the three native engines (copy, the renaming move, the
//! staging move): a clash meets the conflict policy at the RENAMED path, and a
//! cancel, a rollback, or an abrupt end mid-run leaves every item wholly at its
//! original or wholly under its new name, never half-named. The starts and
//! their refusals: `../named_leaves_tests.rs`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::conflict_responder_test_support::ConflictResponderSink;
use super::copy::copy_files_with_progress_inner;
use super::move_op::test_support::{make_state, run_local_move_engine};
use crate::ArmedConflict;
use crate::event_sinks::{CollectorEventSink, OperationEventSink};
use crate::operation_intent::OperationIntent;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::types::{
    ConflictInfo, ConflictResolution, DryRunResult, ScanProgressEvent, TransferLeaves, WriteCancelledEvent,
    WriteCompleteEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    Copy,
    MoveRename,
    MoveStaged,
}

const ENGINES: [Engine; 3] = [Engine::Copy, Engine::MoveRename, Engine::MoveStaged];

fn run(
    engine: Engine,
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    dst: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    match engine {
        Engine::Copy => copy_files_with_progress_inner(events, "named-leaves", state, sources, dst, config),
        Engine::MoveRename => run_local_move_engine(false, events, "named-leaves", state, sources, dst, config),
        Engine::MoveStaged => run_local_move_engine(true, events, "named-leaves", state, sources, dst, config),
    }
}

fn write(path: &Path, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("parents");
    std::fs::write(path, bytes).expect("a fixture file");
    path.to_path_buf()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the folder")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// `src/a.txt` lands as `named.txt`, where an older `named.txt` already sits,
/// beside a bystander under `a.txt`'s ORIGINAL name; `src/b.txt` keeps its name.
/// Every policy meets the clash at `named.txt`, and nothing touches `a.txt`.
#[test]
fn a_clash_at_a_renamed_leaf_meets_the_policy_at_the_renamed_path() {
    let engine = TestOperations::new();
    // (configured policy, what a Stop prompt is answered with)
    let cases = [
        (ConflictResolution::Stop, ConflictResolution::Overwrite),
        (ConflictResolution::Stop, ConflictResolution::Skip),
        (ConflictResolution::Skip, ConflictResolution::Skip),
        (ConflictResolution::Overwrite, ConflictResolution::Overwrite),
    ];
    for kind in ENGINES {
        for (policy, answer) in cases {
            let tmp = TestDir::new("named_leaves_clash");
            let a = write(&tmp.join("src/a.txt"), b"new a");
            let b = write(&tmp.join("src/b.txt"), b"b");
            let dst = tmp.join("dst");
            write(&dst.join("named.txt"), b"old");
            write(&dst.join("a.txt"), b"bystander");
            let config = WriteOperationConfig {
                leaves: TransferLeaves::per_source([(a.clone(), "named.txt".to_string())]),
                conflict_resolution: policy,
                ..WriteOperationConfig::default()
            };
            let state = make_state(&engine, 0);
            let events = ConflictResponderSink::new(&state, answer, false);
            run(kind, &events, &state, &[a.clone(), b.clone()], &dst, &config)
                .unwrap_or_else(|e| panic!("{kind:?} {policy:?}: {e:?}"));

            let ctx = format!("{kind:?} {policy:?}/{answer:?}");
            let prompts = events.inner.conflicts.lock_ignore_poison().clone();
            if policy == ConflictResolution::Stop {
                assert_eq!(prompts.len(), 1, "{ctx}: one prompt");
                assert_eq!(Path::new(&prompts[0].destination_path), dst.join("named.txt"), "{ctx}");
            } else {
                assert!(prompts.is_empty(), "{ctx}: a set policy asks nobody");
            }
            let kept_old = answer == ConflictResolution::Skip;
            let want: &[u8] = if kept_old { b"old" } else { b"new a" };
            assert_eq!(std::fs::read(dst.join("named.txt")).expect("named"), want, "{ctx}");
            assert_eq!(
                std::fs::read(dst.join("a.txt")).expect("bystander"),
                b"bystander",
                "{ctx}"
            );
            assert_eq!(std::fs::read(dst.join("b.txt")).expect("kept name"), b"b", "{ctx}");
            let moved = kind != Engine::Copy;
            assert_eq!(a.exists(), !moved || kept_old, "{ctx}: a skipped move keeps its source");
        }
    }
}

/// How a run is cut short once its first item has landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interruption {
    /// Cancel, keeping what landed.
    Cancel,
    /// Cancel, reversing what landed.
    Rollback,
    /// The thread dies mid-run, the nearest a test gets to a crash.
    Abort,
}

/// A sink that interrupts its operation the first time an item lands.
struct InterruptAfterFirstItem {
    inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    how: Interruption,
    fired: AtomicBool,
}

impl OperationEventSink for InterruptAfterFirstItem {
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
        if self.fired.swap(true, Ordering::SeqCst) {
            return;
        }
        match self.how {
            Interruption::Cancel => self.state.intent().set_for_test(OperationIntent::Stopped),
            Interruption::Rollback => self.state.intent().set_for_test(OperationIntent::RollingBack),
            Interruption::Abort => panic!("the run dies after its first item"),
        }
    }
    fn emit_progress(&self, e: WriteProgressEvent) {
        self.inner.emit_progress(e);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: ArmedConflict) {
        self.inner.emit_conflict(e);
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_scan_progress(&self, e: ScanProgressEvent) {
        self.inner.emit_scan_progress(e);
    }
    fn emit_scan_conflict(&self, c: ConflictInfo) {
        self.inner.emit_scan_conflict(c);
    }
    fn emit_dry_run_complete(&self, r: DryRunResult) {
        self.inner.emit_dry_run_complete(r);
    }
    fn emit_settled(&self, e: WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
}

/// Cut short after its first item, a named run leaves each item wholly at its
/// original or wholly under its new name: never under its old name at the
/// destination, never a part-written or staged name beside it. A rollback that
/// ran puts every original back and leaves the destination as it was.
#[test]
fn an_interrupted_named_run_leaves_nothing_half_named() {
    let engine = TestOperations::new();
    for kind in ENGINES {
        for how in [Interruption::Cancel, Interruption::Rollback, Interruption::Abort] {
            let ctx = format!("{kind:?} {how:?}");
            let tmp = TestDir::new("named_leaves_interrupt");
            let items: Vec<(PathBuf, &str, &[u8])> = vec![
                (write(&tmp.join("src/a.txt"), b"a"), "alpha.txt", b"a"),
                (write(&tmp.join("src/b.txt"), b"b"), "beta.txt", b"b"),
                (write(&tmp.join("src/c.txt"), b"c"), "c.txt", b"c"),
            ];
            let dst = tmp.join("dst");
            std::fs::create_dir_all(&dst).expect("dst");
            let config = WriteOperationConfig {
                leaves: TransferLeaves::per_source([
                    (items[0].0.clone(), "alpha.txt".to_string()),
                    (items[1].0.clone(), "beta.txt".to_string()),
                ]),
                ..WriteOperationConfig::default()
            };
            let sources: Vec<PathBuf> = items.iter().map(|(p, _, _)| p.clone()).collect();
            let state = make_state(&engine, 0);
            let events = InterruptAfterFirstItem {
                inner: CollectorEventSink::new(),
                state: Arc::clone(&state),
                how,
                fired: AtomicBool::new(false),
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| run(kind, &events, &state, &sources, &dst, &config)));
            assert!(events.fired.load(Ordering::SeqCst), "{ctx}: an item landed first");
            match how {
                Interruption::Abort => assert!(outcome.is_err(), "{ctx}: the run died"),
                Interruption::Cancel | Interruption::Rollback => {
                    let result = outcome.expect("no panic");
                    assert!(
                        matches!(result, Err(WriteOperationError::Cancelled { .. }) | Ok(())),
                        "{ctx}: {result:?}"
                    );
                }
            }

            let leaves: Vec<&str> = items.iter().map(|(_, leaf, _)| *leaf).collect();
            // The staging move's op-named folder may outlive the call: a
            // cancelled one clears it as its guard drops, and a dead one keeps
            // it for the next launch's in-flight sweep, as a crash would
            // (`DETAILS.md` § "Durability"). Whatever it holds is named already.
            let staging = dst.join(format!("{}named-leaves", cmdr_fs::staging::STAGING_DIR_PREFIX));
            if let Ok(staged) = std::fs::read_dir(&staging) {
                assert_eq!(kind, Engine::MoveStaged, "{ctx}: only the staging move stages");
                for name in staged.flatten().map(|e| e.file_name().to_string_lossy().into_owned()) {
                    assert!(
                        leaves.contains(&name.as_str()),
                        "{ctx}: staged under its new name: {name}"
                    );
                }
                let _ = std::fs::remove_dir_all(&staging);
            }
            let landed = names_in(&dst);
            for name in &landed {
                assert!(
                    leaves.contains(&name.as_str()),
                    "{ctx}: only whole, named items at the destination: {landed:?}"
                );
            }
            for (source, leaf, bytes) in &items {
                let at_source = std::fs::read(source).ok();
                let at_leaf = std::fs::read(dst.join(leaf)).ok();
                assert!(
                    at_source.as_deref() == Some(*bytes) || at_leaf.as_deref() == Some(*bytes),
                    "{ctx}: {} is wholly somewhere",
                    source.display()
                );
                if let Some(landed) = &at_leaf {
                    assert_eq!(landed.as_slice(), *bytes, "{ctx}: a landed item is whole");
                }
                if kind == Engine::Copy {
                    assert_eq!(
                        at_source.as_deref(),
                        Some(*bytes),
                        "{ctx}: a copy never touches its source"
                    );
                }
            }
            if how == Interruption::Rollback && !events.inner.cancelled.lock_ignore_poison().is_empty() {
                assert!(
                    landed.is_empty(),
                    "{ctx}: a rollback leaves the destination as it was: {landed:?}"
                );
                for (source, _, bytes) in &items {
                    assert_eq!(
                        std::fs::read(source).expect("back"),
                        *bytes,
                        "{ctx}: every original back"
                    );
                }
            }
        }
    }
}
