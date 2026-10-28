//! The gate and the Tags windows: unwritten tag edits ask before a quit, and a tag write running
//! is waited for (bounded) rather than cut between files. Driven against a recorder, like
//! `tests.rs`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use super::*;
use crate::ignore_poison::IgnorePoison;
use crate::tag_editor::at_quit::TagsAtQuit;
use crate::test_support::wait_until;

/// A host with no file operations, only the Tags windows' share, and a record of whether it
/// exited, and whether a tag write was still running when it did.
struct TagsHost {
    tags: Mutex<TagsAtQuit>,
    announced: Mutex<Vec<QuitRequested>>,
    exited: AtomicBool,
    exited_mid_write: AtomicBool,
    /// Bumped by `tags`, so a test can see the teardown checking on the write.
    asked: AtomicUsize,
}

impl TagsHost {
    fn with(tags: TagsAtQuit) -> Arc<Self> {
        Arc::new(Self {
            tags: Mutex::new(tags),
            announced: Mutex::new(Vec::new()),
            exited: AtomicBool::new(false),
            exited_mid_write: AtomicBool::new(false),
            asked: AtomicUsize::new(0),
        })
    }

    fn finish_writing(&self) {
        self.tags.lock_ignore_poison().writing = false;
    }
}

impl QuitHost for TagsHost {
    fn operations(&self) -> Vec<OperationSnapshot> {
        Vec::new()
    }

    fn tags(&self) -> TagsAtQuit {
        self.asked.fetch_add(1, Ordering::SeqCst);
        *self.tags.lock_ignore_poison()
    }

    fn announce(&self, event: QuitRequested) {
        self.announced.lock_ignore_poison().push(event);
    }

    fn announce_called_off(&self) {}
    fn cancel_all(&self) {}
    fn abort_all(&self) {}
    fn flush_temp_ledger(&self) {}

    fn exit(&self) {
        self.exited_mid_write
            .store(self.tags.lock_ignore_poison().writing, Ordering::SeqCst);
        self.exited.store(true, Ordering::SeqCst);
    }
}

fn gate(tag_write_wait: Duration) -> Arc<QuitGate> {
    Arc::new(QuitGate::with_timings(QuitTimings {
        countdown: Duration::from_secs(15),
        drain: Duration::from_millis(20),
        tag_write_wait,
    }))
}

const UNWRITTEN: TagsAtQuit = TagsAtQuit {
    unwritten: 2,
    writing: false,
};
const WRITING: TagsAtQuit = TagsAtQuit {
    unwritten: 1,
    writing: true,
};

#[test]
fn the_decision_holds_for_operations_unwritten_tags_or_a_tag_write() {
    assert!(!holds_quit(&[], TagsAtQuit::default()));
    assert!(holds_quit(&[], UNWRITTEN));
    assert!(holds_quit(&[], WRITING));
}

#[test]
fn unwritten_tag_edits_hold_the_quit_and_say_how_many_windows() {
    let host = TagsHost::with(UNWRITTEN);
    assert!(gate(Duration::from_secs(5)).request_quit(Arc::clone(&host)).is_held());
    let announced = host.announced.lock_ignore_poison().clone();
    assert_eq!(announced.len(), 1);
    assert!(announced[0].operations.is_empty());
    assert_eq!(announced[0].tags, UNWRITTEN);
}

#[test]
fn closed_tags_windows_let_the_quit_through() {
    let host = TagsHost::with(TagsAtQuit::default());
    assert!(!gate(Duration::from_secs(5)).request_quit(Arc::clone(&host)).is_held());
    assert!(host.announced.lock_ignore_poison().is_empty());
}

#[test]
fn a_confirmed_quit_waits_for_the_tag_write_before_it_exits() {
    let host = TagsHost::with(WRITING);
    let gate = gate(Duration::from_secs(10));
    assert!(gate.request_quit(Arc::clone(&host)).is_held());
    assert_eq!(gate.confirm(), QuitAnswer::Answered);

    let asked = host.asked.load(Ordering::SeqCst);
    wait_until(Duration::from_secs(5), "the teardown to check on the write", || {
        host.asked.load(Ordering::SeqCst) > asked + 2
    });
    assert!(!host.exited.load(Ordering::SeqCst), "the write is still running");
    host.finish_writing();
    wait_until(
        Duration::from_secs(5),
        "the quit to go through once the write ends",
        || host.exited.load(Ordering::SeqCst),
    );
    assert!(!host.exited_mid_write.load(Ordering::SeqCst));
}

#[test]
fn a_tag_write_that_never_ends_is_waited_for_only_so_long() {
    let host = TagsHost::with(WRITING);
    let gate = gate(Duration::from_millis(100));
    assert!(gate.request_quit(Arc::clone(&host)).is_held());
    gate.confirm();
    wait_until(Duration::from_secs(5), "the bounded wait to give up and quit", || {
        host.exited.load(Ordering::SeqCst)
    });
}
