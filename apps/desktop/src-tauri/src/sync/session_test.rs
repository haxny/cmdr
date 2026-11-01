//! Sessions end to end on real temp folders: compare, rows, overrides, the
//! confirm's gate, and runs through a test engine of their own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::Volume;
use cmdr_operations::testing::{CollectorEventSink, TestOperations};
use cmdr_operations::{OperationEventSink, Operations};
use cmdr_sync::{
    Acknowledgement, Action, BulkAction, GuardSettings, KeepPlace, KeepPolicy, Mode, ProfileOverrides, RootConflict,
    RootRef, RunItemResult, ShowFilters, Side, SyncDefaults, SyncOptions,
};

use super::engine::SinkFactory;
use super::engine_test::{local_side, test_engine};
use super::host::{SyncEvent, SyncHost};
use super::session::{self, RunLock};
use super::session_compare;
use super::session_run;
use super::wire::{
    CompareOutcome, CompareRequest, ConfirmGroupKey, RunReportWire, RunRequest, SelectionRef, SideRequest, SyncError,
    ViewRowWire,
};

struct TestHost {
    engine: &'static TestOperations,
    volumes: Mutex<HashMap<String, Arc<dyn Volume>>>,
    events: Mutex<Vec<SyncEvent>>,
    sink: Arc<CollectorEventSink>,
}

impl SyncHost for TestHost {
    fn volume(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        self.volumes.lock_ignore_poison().get(volume_id).cloned()
    }
    fn selection_names(&self, _root: &RootRef, selection: &SelectionRef, _hidden: bool) -> Option<Vec<String>> {
        (selection.expected_sequence == 7).then(|| vec!["a.txt".to_owned()])
    }
    fn operations(&self) -> Option<&'static Operations> {
        let engine: &'static Operations = self.engine;
        Some(engine)
    }
    fn sinks(&self) -> SinkFactory {
        let sink = Arc::clone(&self.sink);
        Arc::new(move || Arc::clone(&sink) as Arc<dyn OperationEventSink>)
    }
    fn emit(&self, event: SyncEvent) {
        self.events.lock_ignore_poison().push(event);
    }
}

struct Pair {
    _dir: TestDir,
    host: Arc<TestHost>,
    left: PathBuf,
    right: PathBuf,
    left_ref: RootRef,
    right_ref: RootRef,
}

fn pair(label: &str) -> Pair {
    let dir = TestDir::new(label);
    let (left, right) = (dir.join("L"), dir.join("R"));
    std::fs::create_dir_all(&left).expect("left");
    std::fs::create_dir_all(&right).expect("right");
    let engine = test_engine();
    let mut volumes = HashMap::new();
    let mut root_ref = |tag: &str, root: &Path| {
        let side = local_side(engine, tag, root);
        volumes.insert(side.volume_id.clone(), side.volume);
        RootRef {
            volume_id: side.volume_id,
            path: root.to_string_lossy().into_owned(),
        }
    };
    let (left_ref, right_ref) = (root_ref("sync-left", &left), root_ref("sync-right", &right));
    let local = local_side(engine, "sync-local-disk", Path::new("/"));
    volumes.insert("root".to_owned(), local.volume);
    let host = Arc::new(TestHost {
        engine,
        volumes: Mutex::new(volumes),
        events: Mutex::new(Vec::new()),
        sink: Arc::new(CollectorEventSink::new()),
    });
    Pair {
        _dir: dir,
        host,
        left,
        right,
        left_ref,
        right_ref,
    }
}

fn defaults() -> SyncDefaults {
    SyncDefaults {
        // Versions folders only: nothing of a test lands in the real Trash.
        keep: KeepPolicy {
            place: KeepPlace::VersionsAlways,
            ..KeepPolicy::default()
        },
        time_tolerance_secs: 2,
        ignore_hour_shift: false,
        guards: GuardSettings {
            max_remove_percent: 50,
            max_remove_count: 0,
            sentinel: None,
        },
    }
}

fn all_rows() -> ShowFilters {
    ShowFilters {
        to_right: true,
        equal: true,
        different: true,
        to_left: true,
        duplicates: true,
        singles: true,
        unknown: true,
        warnings_only: false,
    }
}

fn write(path: &Path, content: &str, age_secs: u64) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, content).expect("write");
    let file = std::fs::File::options().write(true).open(path).expect("open");
    let when = SystemTime::now() - Duration::from_secs(age_secs);
    file.set_modified(when).expect("mtime");
}

impl Pair {
    fn request(&self, mode: Mode) -> CompareRequest {
        CompareRequest {
            session_id: None,
            left: SideRequest {
                root: self.left_ref.clone(),
                selection: None,
            },
            right: SideRequest {
                root: self.right_ref.clone(),
                selection: None,
            },
            options: SyncOptions {
                mode,
                ..SyncOptions::default()
            },
            defaults: defaults(),
            overrides: ProfileOverrides::default(),
            profile_name: None,
            include_hidden: true,
        }
    }

    fn compare_finished(&self, id: &str) -> Option<CompareOutcome> {
        self.host
            .events
            .lock_ignore_poison()
            .iter()
            .find_map(|event| match event {
                SyncEvent::CompareFinished(done) if done.session_id == id => Some(done.outcome.clone()),
                _ => None,
            })
    }

    async fn compare(&self, request: CompareRequest) -> String {
        let id = session_compare::start(Arc::clone(&self.host) as Arc<dyn SyncHost>, request).expect("starts");
        wait_until_async(Duration::from_secs(10), "the compare finishes", || {
            self.compare_finished(&id).is_some()
        })
        .await;
        assert_eq!(self.compare_finished(&id), Some(CompareOutcome::Done));
        id
    }

    fn run_finished(&self, id: &str) -> Option<RunReportWire> {
        self.host
            .events
            .lock_ignore_poison()
            .iter()
            .rev()
            .find_map(|event| match event {
                SyncEvent::RunFinished(done) if done.session_id == id => Some(done.report.clone()),
                _ => None,
            })
    }

    async fn run(&self, id: &str, groups: &[ConfirmGroupKey], acks: &[Acknowledgement]) -> RunReportWire {
        let runs_before = self.runs(id);
        let request = RunRequest {
            groups: groups.to_vec(),
            acknowledged: acks.to_vec(),
            defaults: defaults(),
        };
        let host = Arc::clone(&self.host) as Arc<dyn SyncHost>;
        session_run::start_run(host, id, &request).expect("the run starts");
        wait_until_async(Duration::from_secs(20), "the run finishes", || {
            self.runs(id) > runs_before
        })
        .await;
        self.run_finished(id).expect("a report")
    }

    fn runs(&self, id: &str) -> usize {
        let events = self.host.events.lock_ignore_poison();
        events
            .iter()
            .filter(|event| matches!(event, SyncEvent::RunFinished(done) if done.session_id == id))
            .count()
    }

    fn index_of(&self, id: &str, name: &str) -> u32 {
        let page = session::rows(id, 0, 10_000, &all_rows()).expect("rows");
        page.rows
            .iter()
            .find_map(|row| match &row.row {
                ViewRowWire::Item {
                    index,
                    left_name,
                    right_name,
                    ..
                } if left_name.as_deref() == Some(name) || right_name.as_deref() == Some(name) => Some(*index),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no row for {name}"))
    }

    fn result(report: &RunReportWire, index: u32) -> Option<RunItemResult> {
        report
            .items
            .iter()
            .find(|item| item.item == index)
            .map(|item| item.result)
    }
}

/// Every file under `root`'s versions folders, by name.
fn kept_names(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if let Some(name) = path.file_name() {
                found.push(name.to_string_lossy().into_owned());
            }
        }
    }
    let mut found = Vec::new();
    walk(&root.join(".cmdr-sync/versions"), &mut found);
    found.sort();
    found
}

const ALL_GROUPS: [ConfirmGroupKey; 8] = [
    ConfirmGroupKey::CopyRight,
    ConfirmGroupKey::CopyLeft,
    ConfirmGroupKey::CopyLeftRecreates,
    ConfirmGroupKey::CopyRightRecreates,
    ConfirmGroupKey::ReplaceRight,
    ConfirmGroupKey::ReplaceLeft,
    ConfirmGroupKey::RemoveRight,
    ConfirmGroupKey::RemoveLeft,
];

#[tokio::test]
async fn compare_rows_override_and_state_round_trip() {
    let p = pair("sync_session_round_trip");
    write(&p.left.join("a.txt"), "a", 100);
    write(&p.left.join("d/b.txt"), "b", 100);
    let id = p.compare(p.request(Mode::BothWays)).await;
    let a = p.index_of(&id, "a.txt");
    let page = session::rows(&id, 0, 100, &all_rows()).expect("rows");
    assert!(page.rows.iter().any(|row| matches!(&row.row,
        ViewRowWire::Item { index, action: Action::CopyRight, default_action: Action::CopyRight, .. } if *index == a)));
    let before = session::state(&id, &all_rows()).expect("state");
    assert_eq!(before.summary.into_right.copy_files, 2);
    let after = session::set_actions(&id, &[a], BulkAction::Skip, &all_rows()).expect("override");
    assert_eq!(after.summary.into_right.copy_files, 1);
    assert_eq!(after.summary.changed_by_you, 1);
    let page = session::rows(&id, 0, 100, &all_rows()).expect("rows");
    assert!(page.rows.iter().any(|row| matches!(&row.row,
        ViewRowWire::Item { index, action: Action::Skip, default_action: Action::CopyRight, .. } if *index == a)));
    let refused = session::set_actions(&id, &[999], BulkAction::Skip, &all_rows());
    assert!(matches!(refused, Err(SyncError::Override { .. })));
    session::close(&id);
    assert!(matches!(
        session::state(&id, &all_rows()),
        Err(SyncError::NoSuchSession)
    ));
}

#[tokio::test]
async fn compare_refuses_nested_roots() {
    let p = pair("sync_session_nested");
    let mut request = p.request(Mode::Mirror);
    request.right.root = RootRef {
        volume_id: p.left_ref.volume_id.clone(),
        path: p.left.join("inner").to_string_lossy().into_owned(),
    };
    let error = session_compare::start(Arc::clone(&p.host) as Arc<dyn SyncHost>, request).expect_err("refused");
    assert!(matches!(
        error,
        SyncError::RootsConflict {
            conflict: RootConflict::Nested { outer: Side::Left }
        }
    ));
}

#[tokio::test]
async fn missing_volume_is_volume_not_connected_and_bad_inputs_are_refused_by_type() {
    let p = pair("sync_session_inputs");
    let host = || Arc::clone(&p.host) as Arc<dyn SyncHost>;
    let mut request = p.request(Mode::Update);
    request.right.root.volume_id = "gone".to_owned();
    let error = session_compare::start(host(), request).expect_err("refused");
    assert_eq!(
        error,
        SyncError::VolumeNotConnected {
            side: Side::Right,
            volume_id: "gone".to_owned()
        }
    );
    let mut request = p.request(Mode::Update);
    request.left.root.path = format!("{}/../R", p.left.display());
    assert_eq!(
        session_compare::start(host(), request),
        Err(SyncError::BadPath { side: Side::Left })
    );
    let mut request = p.request(Mode::Update);
    request.defaults.guards.max_remove_percent = 100;
    assert!(matches!(
        session_compare::start(host(), request),
        Err(SyncError::BadSettings { .. })
    ));
    let mut request = p.request(Mode::Update);
    request.options.only_selected = true;
    request.left.selection = Some(SelectionRef {
        listing_id: "pane".to_owned(),
        indices: vec![0],
        expected_sequence: 3,
    });
    assert_eq!(
        session_compare::start(host(), request),
        Err(SyncError::StaleSelection { side: Side::Left })
    );
    let mut request = p.request(Mode::Update);
    request.session_id = Some("no-such-session".to_owned());
    assert_eq!(session_compare::start(host(), request), Err(SyncError::NoSuchSession));
}

#[tokio::test]
async fn second_run_on_the_same_pair_is_refused() {
    let p = pair("sync_session_run_lock");
    write(&p.left.join("a.txt"), "a", 100);
    let id = p.compare(p.request(Mode::Update)).await;
    // Another session holds the pair (in either order).
    let held = RunLock::take(&p.right_ref, &p.left_ref).expect("first lock");
    let request = RunRequest {
        groups: ALL_GROUPS.to_vec(),
        acknowledged: Vec::new(),
        defaults: defaults(),
    };
    let host = Arc::clone(&p.host) as Arc<dyn SyncHost>;
    let refused = session_run::start_run(host, &id, &request);
    assert!(matches!(refused, Err(SyncError::RunInProgress)));
    drop(held);
    // The right root is empty: copies into it wait for "Copy anyway".
    let report = p.run(&id, &ALL_GROUPS, &[]).await;
    assert_eq!((report.done, report.held), (0, 1));
    let report = p.run(&id, &ALL_GROUPS, &[Acknowledgement::CopyIntoEmpty]).await;
    assert_eq!(report.done, 1);
}

#[tokio::test]
async fn run_rechecks_guards_on_the_overridden_plan_and_drops_unaccepted_groups() {
    let p = pair("sync_session_gate");
    write(&p.left.join("new.txt"), "n", 100);
    write(&p.left.join("k.txt"), "k", 100);
    write(&p.right.join("k.txt"), "k", 100);
    for name in ["x.txt", "y.txt", "z.txt"] {
        write(&p.right.join(name), name, 100);
    }
    let id = p.compare(p.request(Mode::Mirror)).await;
    let preview = session_run::confirm(&id, &defaults(), &all_rows()).expect("confirm");
    assert!(
        !preview.blocks.is_empty(),
        "three of four files removed trips max remove"
    );

    // Unticked copies don't run; held removals stay.
    let report = p.run(&id, &[ConfirmGroupKey::RemoveRight], &[]).await;
    assert_eq!(report.held, 3);
    assert_eq!(report.done, 0);
    assert!(!p.right.join("new.txt").exists());
    assert!(p.right.join("x.txt").exists());

    // Two removals skipped: one of four is under the limit, so the gate at run
    // time (not the confirm's earlier report) lets it through.
    let id = p.compare(p.request(Mode::Mirror)).await;
    let (x, y) = (p.index_of(&id, "x.txt"), p.index_of(&id, "y.txt"));
    session::set_actions(&id, &[x, y], BulkAction::Skip, &all_rows()).expect("override");
    let report = p.run(&id, &ALL_GROUPS, &[]).await;
    assert_eq!(report.held, 0);
    assert!(p.right.join("x.txt").exists() && p.right.join("y.txt").exists());
    assert!(!p.right.join("z.txt").exists());
    assert!(p.right.join("new.txt").exists());
    assert_eq!(kept_names(&p.right), vec!["z.txt".to_owned()]);
}

#[tokio::test]
async fn both_ways_run_copies_each_way_and_keeps_what_it_replaced() {
    let p = pair("sync_session_both_ways");
    write(&p.left.join("a.txt"), "left only", 100);
    write(&p.right.join("b.txt"), "right only", 100);
    write(&p.left.join("c.txt"), "newer on the left", 10);
    write(&p.right.join("c.txt"), "older", 500);
    let id = p.compare(p.request(Mode::BothWays)).await;
    let report = p.run(&id, &ALL_GROUPS, &[]).await;
    assert_eq!(report.done, 3, "{report:?}");
    assert_eq!(std::fs::read_to_string(p.right.join("a.txt")).expect("a"), "left only");
    assert_eq!(std::fs::read_to_string(p.left.join("b.txt")).expect("b"), "right only");
    assert_eq!(
        std::fs::read_to_string(p.right.join("c.txt")).expect("c"),
        "newer on the left"
    );
    assert_eq!(kept_names(&p.right), vec!["c.txt".to_owned()]);
    assert!(!report.operation_ids.is_empty());
    let state = session::state(&id, &all_rows()).expect("state");
    assert!(state.stale);
}

#[tokio::test]
async fn update_run_skips_a_source_changed_after_compare() {
    let p = pair("sync_session_update");
    write(&p.left.join("a.txt"), "a", 100);
    write(&p.left.join("d.txt"), "d", 100);
    let id = p.compare(p.request(Mode::Update)).await;
    let d = p.index_of(&id, "d.txt");
    write(&p.left.join("d.txt"), "d, edited", 5);
    let report = p.run(&id, &ALL_GROUPS, &[Acknowledgement::CopyIntoEmpty]).await;
    assert_eq!(Pair::result(&report, d), Some(RunItemResult::ChangedSinceCompare));
    assert_eq!(report.done, 1);
    assert!(p.right.join("a.txt").exists());
    assert!(!p.right.join("d.txt").exists());
}

#[tokio::test]
async fn mirror_run_keeps_every_replaced_and_removed_file_and_leaves_a_changed_one() {
    let p = pair("sync_session_mirror");
    write(&p.left.join("c.txt"), "new c", 10);
    write(&p.right.join("c.txt"), "old c", 500);
    write(&p.right.join("old.txt"), "old", 100);
    write(&p.right.join("e.txt"), "e", 100);
    let id = p.compare(p.request(Mode::Mirror)).await;
    let e = p.index_of(&id, "e.txt");
    write(&p.right.join("e.txt"), "e, edited after compare", 3);
    let report = p.run(&id, &ALL_GROUPS, &[Acknowledgement::RemoveAnyway]).await;
    assert_eq!(Pair::result(&report, e), Some(RunItemResult::ChangedSinceCompare));
    assert_eq!(report.done, 2, "{report:?}");
    assert_eq!(std::fs::read_to_string(p.right.join("c.txt")).expect("c"), "new c");
    assert!(!p.right.join("old.txt").exists());
    assert_eq!(
        std::fs::read_to_string(p.right.join("e.txt")).expect("e"),
        "e, edited after compare"
    );
    // Nothing destroyed: the replaced and the removed file are both kept.
    assert_eq!(kept_names(&p.right), vec!["c.txt".to_owned(), "old.txt".to_owned()]);
    assert_eq!(report.versions_folders.len(), 1);
    assert!(report.kept_bytes > 0);
}
