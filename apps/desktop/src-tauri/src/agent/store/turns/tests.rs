//! The turn-scoped reads: the rows around a page that complete its turns, and the rows that hold
//! one assistant row's results.

use rusqlite::Connection;

use super::*;
use crate::agent::llm::types::AgentPart;
use crate::agent::store::{append_message, create_conversation};

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    super::super::run_migrations(&conn, super::super::MIGRATIONS).expect("migrate");
    conn
}

/// Append one row of `role`, returning its id.
fn row(conn: &Connection, conversation_id: i64, role: AgentRole) -> i64 {
    append_message(
        conn,
        conversation_id,
        role,
        &[AgentPart::Text("x".into())],
        "",
        None,
        None,
        100,
    )
    .expect("append")
    .0
}

/// Two turns: user, assistant, tool, assistant (seq 0–3), then user, assistant, tool (seq 4–6).
fn two_turns(conn: &Connection) -> (i64, Vec<i64>) {
    let id = create_conversation(conn, "t", 100, None).expect("create");
    let roles = [
        AgentRole::User,
        AgentRole::Assistant,
        AgentRole::Tool,
        AgentRole::Assistant,
        AgentRole::User,
        AgentRole::Assistant,
        AgentRole::Tool,
    ];
    let ids = roles.into_iter().map(|role| row(conn, id, role)).collect();
    (id, ids)
}

fn ids(rows: &[StoredMessage]) -> Vec<i64> {
    rows.iter().map(|row| row.id).collect()
}

#[test]
fn a_page_starting_and_ending_mid_turn_gets_the_rest_of_both_turns() {
    let conn = migrated_conn();
    let (id, rows) = two_turns(&conn);

    // The page is seq 2..=5: it starts after the first opener and ends before the last row.
    let context = turn_context(&conn, id, 2, 5).expect("context");
    assert_eq!(ids(&context.before), rows[0..2], "back to the first turn's opener");
    assert_eq!(ids(&context.after), rows[6..7], "on to the end of the last turn");
}

#[test]
fn a_page_starting_at_an_opener_needs_nothing_before_it() {
    let conn = migrated_conn();
    let (id, _) = two_turns(&conn);

    let context = turn_context(&conn, id, 4, 6).expect("context");
    assert!(context.before.is_empty());
    assert!(context.after.is_empty());
    // And a page ending just before an opener stops there: the next turn isn't this page's.
    let context = turn_context(&conn, id, 0, 3).expect("context");
    assert!(context.after.is_empty());
}

#[test]
fn rows_before_the_first_opener_have_no_turn_to_complete() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "t", 100, None).expect("create");
    row(&conn, id, AgentRole::Assistant);
    row(&conn, id, AgentRole::Assistant);

    let context = turn_context(&conn, id, 1, 1).expect("context");
    assert!(context.before.is_empty(), "no opener to reach back to");
    assert_eq!(turn_opener_at(&conn, id, 1).expect("opener"), None);
}

#[test]
fn the_opener_at_a_row_is_the_latest_user_row_at_or_before_it() {
    let conn = migrated_conn();
    let (id, rows) = two_turns(&conn);

    assert_eq!(turn_opener_at(&conn, id, 0).expect("opener"), Some(rows[0]));
    assert_eq!(turn_opener_at(&conn, id, 3).expect("opener"), Some(rows[0]));
    assert_eq!(turn_opener_at(&conn, id, 6).expect("opener"), Some(rows[4]));
}

#[test]
fn an_assistant_row_reads_with_the_rest_of_its_turn() {
    let conn = migrated_conn();
    let (id, rows) = two_turns(&conn);

    let read = row_and_rest_of_turn(&conn, id, rows[1]).expect("read").expect("found");
    assert_eq!(ids(&read), rows[1..4], "from the row up to the next opener");
    let read = row_and_rest_of_turn(&conn, id, rows[5]).expect("read").expect("found");
    assert_eq!(ids(&read), rows[5..7], "the last turn runs to the thread's end");
}

#[test]
fn another_threads_row_is_not_found() {
    let conn = migrated_conn();
    let (_, rows) = two_turns(&conn);
    let other = create_conversation(&conn, "other", 100, None).expect("create");

    assert!(row_and_rest_of_turn(&conn, other, rows[1]).expect("read").is_none());
    assert!(row_and_rest_of_turn(&conn, other, 999_999).expect("read").is_none());
}
