//! A session's life around its owner: closing, the window it belongs to, a
//! compare that asked something else, single-row compares, and page sizes.

use std::sync::Arc;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_sync::{Mode, SyncControl, SyncOptions};

use super::session::{self, MAX_ROWS_PAGE, owned_session, root_path_ok};
use super::session_compare;
use super::session_run;
use super::session_test::{ALL_GROUPS, CLOSED_WINDOW, all_rows, pair, write};
use super::wire::{CompareRequest, SyncError};

#[tokio::test]
async fn closing_marks_the_session_and_stops_its_single_row_compares() {
    let p = pair("sync_session_close");
    write(&p.left.join("a.txt"), "a", 100);
    let id = p.compare(p.request(Mode::Update)).await;
    let held = session::session(&id).expect("open");
    let row = Arc::new(SyncControl::new());
    held.row_compares.lock_ignore_poison().push(Arc::clone(&row));
    session::close(&id);
    assert!(held.is_closed());
    assert!(row.is_cancelled(), "close reaches a single-row compare");
    let again = CompareRequest {
        session_id: Some(id.clone()),
        ..p.request(Mode::Update)
    };
    let refused = session_compare::start(p.host(), again, None).await;
    assert_eq!(refused.err(), Some(SyncError::NoSuchSession));
}

#[tokio::test]
async fn a_compare_for_a_window_that_closed_leaves_no_session() {
    let p = pair("sync_session_window_gone");
    let started = session_compare::start(p.host(), p.request(Mode::Update), Some(CLOSED_WINDOW.to_owned())).await;
    assert_eq!(started.err(), Some(SyncError::NoSuchSession));
}

#[tokio::test]
async fn only_the_owning_window_may_change_a_session() {
    let p = pair("sync_session_owner");
    let started = session_compare::start(p.host(), p.request(Mode::Update), Some("main".to_owned()))
        .await
        .expect("starts");
    let id = started.session_id;
    assert!(owned_session(&id, "main").is_ok());
    assert_eq!(owned_session(&id, "other").err(), Some(SyncError::NoSuchSession));
    let from_other = CompareRequest {
        session_id: Some(id.clone()),
        ..p.request(Mode::Update)
    };
    let refused = session_compare::start(p.host(), from_other, Some("other".to_owned())).await;
    assert_eq!(refused.err(), Some(SyncError::NoSuchSession));
    session::close(&id);
}

#[tokio::test]
async fn a_refused_compare_that_asked_something_else_leaves_the_old_plan_stale() {
    let p = pair("sync_session_refused_recompare");
    write(&p.left.join("a.txt"), "a", 100);
    let id = p.compare(p.request(Mode::Update)).await;
    let request = p.run_request(&id, &ALL_GROUPS, &[]).await;
    let other = CompareRequest {
        session_id: Some(id.clone()),
        options: SyncOptions {
            masks: "\"".to_owned(),
            ..p.request(Mode::Mirror).options
        },
        ..p.request(Mode::Mirror)
    };
    let refused = session_compare::start(p.host(), other, None).await;
    assert!(matches!(refused, Err(SyncError::BadMask { .. })), "{refused:?}");
    let run = session_run::start_run(p.host(), &id, &request).await;
    assert_eq!(run.err(), Some(SyncError::StaleCompare));
    session::close(&id);
}

#[tokio::test]
async fn a_page_of_rows_is_capped() {
    let p = pair("sync_session_page_cap");
    let count = MAX_ROWS_PAGE as usize + 5;
    for i in 0..count {
        std::fs::write(p.left.join(format!("f{i:05}.txt")), "x").expect("write");
    }
    let id = p.compare(p.request(Mode::Update)).await;
    let page = session::rows(&id, 0, u32::MAX, &all_rows()).expect("rows");
    assert_eq!(page.total as usize, count);
    assert_eq!(page.rows.len(), MAX_ROWS_PAGE as usize);
    let rest = session::rows(&id, MAX_ROWS_PAGE, u32::MAX, &all_rows()).expect("rows");
    assert_eq!(rest.rows.len(), 5);
    session::close(&id);
}

#[test]
fn a_root_must_be_an_absolute_path_that_never_climbs() {
    assert!(root_path_ok("/Users/me"));
    assert!(root_path_ok("/"));
    assert!(!root_path_ok("Users/me"));
    assert!(!root_path_ok(""));
    assert!(!root_path_ok("/a/../b"));
}
