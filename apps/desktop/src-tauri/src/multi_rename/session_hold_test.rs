//! Holding a session for a Tags window (`hold` / `release`), and copying one for it (`reopen`):
//! a held session survives the idle limit and the cap; released, it ages like any other.

use super::*;

fn session(last_used: Instant) -> Session {
    Session {
        listing_id: String::new(),
        volume_id: "root".to_string(),
        dir: PathBuf::new(),
        names: Vec::new().into(),
        edits: Arc::default(),
        written: None,
        next_preview: 1,
        latest: None,
        last_used,
        media: Arc::new(SessionMedia::seed("", PathBuf::new(), &[], &[])),
        held: false,
    }
}

fn insert_for_test(id: &str, last_used: Instant) {
    SESSIONS.lock_ignore_poison().insert(id.to_string(), session(last_used));
}

fn held(last_used: Instant) -> Session {
    let mut session = session(last_used);
    session.held = true;
    session
}

#[test]
fn a_held_session_survives_the_idle_limit_and_never_counts_against_the_cap() {
    let now = Instant::now();
    let mut sessions = HashMap::from([
        ("held-idle".to_string(), held(now - IDLE_LIMIT - Duration::from_secs(1))),
        ("held-oldest".to_string(), held(now - Duration::from_secs(120))),
        ("old".to_string(), session(now - Duration::from_secs(60))),
        ("newest".to_string(), session(now)),
    ]);

    // Two unheld sessions fit under 3 with room for one more: nothing goes.
    evict_to(&mut sessions, now, 3);
    assert_eq!(sessions.len(), 4);

    // Under 2, the oldest UNHELD one goes; the held ones are never candidates.
    evict_to(&mut sessions, now, 2);
    let mut left: Vec<&str> = sessions.keys().map(String::as_str).collect();
    left.sort_unstable();
    assert_eq!(left, vec!["held-idle", "held-oldest", "newest"]);
}

#[test]
fn open_tags_windows_never_push_out_the_sheets_live_session() {
    // Three Tags windows hold their sessions and the sheet's session is open: the next open
    // (the sheet's "Tags…" reopening it) must keep the sheet's.
    let now = Instant::now();
    let mut sessions = HashMap::from([
        ("tags-1".to_string(), held(now - Duration::from_secs(300))),
        ("tags-2".to_string(), held(now - Duration::from_secs(200))),
        ("tags-3".to_string(), held(now - Duration::from_secs(100))),
        ("sheet".to_string(), session(now - Duration::from_secs(400))),
    ]);

    evict_to(&mut sessions, now, MAX_SESSIONS_IN_PRODUCTION);

    assert!(sessions.contains_key("sheet"));
    assert_eq!(sessions.len(), 4);
}

#[test]
fn the_cap_stops_at_held_sessions_rather_than_dropping_one() {
    let now = Instant::now();
    let mut sessions = HashMap::from([
        ("a".to_string(), held(now - Duration::from_secs(60))),
        ("b".to_string(), held(now)),
    ]);

    evict_to(&mut sessions, now, 1);

    assert_eq!(sessions.len(), 2);
}

#[test]
fn hold_and_release_flip_the_stored_session() {
    insert_for_test("hold-me", Instant::now());
    hold("hold-me").expect("the session is open");
    assert!(SESSIONS.lock_ignore_poison()["hold-me"].held);

    release("hold-me");
    assert!(!SESSIONS.lock_ignore_poison()["hold-me"].held);

    // Released, it ages like any other: past the idle limit the next open drops it.
    let mut sessions = HashMap::from([(
        "hold-me".to_string(),
        SESSIONS.lock_ignore_poison().remove("hold-me").expect("still stored"),
    )]);
    let later = Instant::now() + IDLE_LIMIT + Duration::from_secs(1);
    evict_to(&mut sessions, later, 5);
    assert!(sessions.is_empty());
}

#[test]
fn holding_a_closed_session_says_so_and_releasing_one_is_a_no_op() {
    assert!(matches!(hold("never-opened"), Err(MultiRenameError::SessionClosed)));
    release("never-opened");
}

#[test]
fn reopen_copies_the_folder_and_the_files_in_their_current_order_into_a_new_session() {
    insert_for_test("reopen-source", Instant::now());
    {
        let mut sessions = SESSIONS.lock_ignore_poison();
        let source = sessions.get_mut("reopen-source").expect("inserted");
        source.dir = PathBuf::from("/books/dracula");
        source.names = vec!["02.mp3".to_string(), "01.mp3".to_string()].into();
    }

    let copy = reopen("reopen-source").expect("the source is open");

    assert_ne!(copy.session_id, "reopen-source");
    assert_eq!(copy.count, 2);
    let files = files_of(&copy.session_id).expect("the copy is open");
    assert_eq!(files.dir, PathBuf::from("/books/dracula"));
    assert_eq!(&*files.names, ["02.mp3".to_string(), "01.mp3".to_string()]);
    // Closing the sheet's session leaves the copy alone.
    close("reopen-source");
    assert!(is_open(&copy.session_id));
    close(&copy.session_id);
    assert!(matches!(reopen("reopen-source"), Err(MultiRenameError::SessionClosed)));
}
