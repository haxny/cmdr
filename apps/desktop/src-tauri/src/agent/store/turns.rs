//! Reads scoped to TURNS rather than to a page of rows.
//!
//! A turn is every row from one user-role row (its opener) up to the next one. What a tool line
//! says (its result, whether it repeats an earlier call, what the turn spent) is turn-scoped,
//! while the history pages by rows, so a page that starts or ends mid-turn needs the rows around
//! it that share its turns. These reads find them; `agent::chat::message_view` decides what they
//! mean.

use rusqlite::Connection;

use super::AgentStoreError;
use super::query::{MESSAGE_COLUMNS, StoredMessage, map_message_row};
use crate::agent::llm::types::AgentRole;

/// The rows that share a turn with a page but sit outside it: `before` runs from the opener of
/// the page's first turn up to its first row (empty when the page starts at an opener, or before
/// the thread's first user row), and `after` from past its last row up to the next opener.
#[derive(Debug, Default)]
pub struct TurnContext {
    pub before: Vec<StoredMessage>,
    pub after: Vec<StoredMessage>,
}

/// The rows around the page `first_seq..=last_seq` that complete its first and last turns.
pub fn turn_context(
    conn: &Connection,
    conversation_id: i64,
    first_seq: i64,
    last_seq: i64,
) -> Result<TurnContext, AgentStoreError> {
    let user = AgentRole::User.as_token();
    let before = rows_where(
        conn,
        "seq < ?2 AND seq >= COALESCE(
             (SELECT MAX(seq) FROM messages WHERE conversation_id = ?1 AND role = ?3 AND seq <= ?2), ?2)",
        rusqlite::params![conversation_id, first_seq, user],
    )?;
    let after = rows_where(
        conn,
        "seq > ?2 AND seq < COALESCE(
             (SELECT MIN(seq) FROM messages WHERE conversation_id = ?1 AND role = ?3 AND seq > ?2), ?4)",
        rusqlite::params![conversation_id, last_seq, user, i64::MAX],
    )?;
    Ok(TurnContext { before, after })
}

/// The id of the opener of the turn the row at `seq` belongs to: the latest user-role row at or
/// before it. `None` before the thread's first user row.
pub fn turn_opener_at(conn: &Connection, conversation_id: i64, seq: i64) -> Result<Option<i64>, AgentStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM messages WHERE conversation_id = ?1 AND role = ?2 AND seq <= ?3 ORDER BY seq DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(rusqlite::params![conversation_id, AgentRole::User.as_token(), seq])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// The row `message_id` of `conversation_id` plus every row after it up to the next opener: the
/// rows holding an assistant row's results. `None` when that thread holds no such row, so another
/// thread's id never reads.
pub fn row_and_rest_of_turn(
    conn: &Connection,
    conversation_id: i64,
    message_id: i64,
) -> Result<Option<Vec<StoredMessage>>, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT seq FROM messages WHERE id = ?1 AND conversation_id = ?2")?;
    let mut found = stmt.query(rusqlite::params![message_id, conversation_id])?;
    let Some(row) = found.next()? else {
        return Ok(None);
    };
    let seq: i64 = row.get(0)?;
    let rows = rows_where(
        conn,
        "seq >= ?2 AND seq < COALESCE(
             (SELECT MIN(seq) FROM messages WHERE conversation_id = ?1 AND role = ?3 AND seq > ?2), ?4)",
        rusqlite::params![conversation_id, seq, AgentRole::User.as_token(), i64::MAX],
    )?;
    Ok(Some(rows))
}

/// A conversation's rows matching `filter` (over `seq`, with `?1` bound to the conversation),
/// seq ascending.
fn rows_where(
    conn: &Connection,
    filter: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<StoredMessage>, AgentStoreError> {
    let sql =
        format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE conversation_id = ?1 AND {filter} ORDER BY seq ASC");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(params)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_message_row(row)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
