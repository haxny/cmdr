//! The one line a decided group gets in its sweep's thread, and the group's pointer to it.
//!
//! An approval's line exists twice over its life: "running" when the operation starts, then
//! the outcome when it settles. Both are the SAME `messages` row (`proposals.thread_line_id`),
//! rewritten in place, so the thread keeps one line per decision and a rail that already shows
//! the row patches it rather than adding a second.
//!
//! ⚠️ **Each write checks the pointer and writes the row in one `BEGIN IMMEDIATE`.** The start
//! and the settle run on different connections, and a quick operation can settle before its
//! start is reported. Under the lock, whichever comes second sees the first: a late "running"
//! finds a line and stays out, and a settle finds the running line and rewrites it.

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use super::super::AgentStoreError;
use super::super::events::{ConversationEvent, EVENT_ROLE_TOKEN};
use super::super::rows::insert_message_row_in;
use super::read::read_server_proposal;
use super::{ProposalGroup, decode_token};
use crate::agent::types::{ProposalDecision, ProposalOutcomeKind, ProposalStatus};

/// A line written: its `messages` row and that row's place in the thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadLine {
    pub message_id: i64,
    pub seq: i64,
}

/// A line that still says "running", found at launch, with what its group became since.
#[derive(Debug, Clone)]
pub struct RunningLine {
    pub group_id: i64,
    pub message_id: i64,
    pub status: ProposalStatus,
    pub decision: ProposalDecision,
}

/// Write the "running" line of a group whose operation just started.
///
/// `None` when it has no business existing: the group already has a line (its settle got
/// there first), or it is no longer `approved` (settled, or given back). Either way the
/// thread already says what's true.
pub fn open_running_line(
    conn: &Connection,
    group_id: i64,
    conversation_id: i64,
    decision: ProposalDecision,
    now: i64,
) -> Result<Option<ThreadLine>, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let Some((status, line)) = pointer(&tx, group_id)? else {
        return Ok(None);
    };
    if status != ProposalStatus::Approved.as_token() || line.is_some() {
        return Ok(None);
    }
    let written = append_line(&tx, group_id, conversation_id, decision, now)?;
    tx.commit()?;
    Ok(Some(written))
}

/// Write a group's final line: rewrite the one it already has, or append one.
///
/// A rewrite keeps the row's id and `seq`, so the line stays where the user acted and the
/// announcement names a row the rail may already show.
pub fn settle_line(
    conn: &Connection,
    group_id: i64,
    conversation_id: i64,
    decision: ProposalDecision,
    now: i64,
) -> Result<ThreadLine, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let line = pointer(&tx, group_id)?.and_then(|(_, line)| line);
    let written = match line {
        Some(message_id) => match rewrite(&tx, message_id, decision.clone())? {
            Some(seq) => ThreadLine { message_id, seq },
            None => append_line(&tx, group_id, conversation_id, decision, now)?,
        },
        None => append_line(&tx, group_id, conversation_id, decision, now)?,
    };
    tx.commit()?;
    Ok(written)
}

/// Every line still saying "running", with its group's status. At launch nothing is running, so
/// each one is a line whose settle never landed.
pub fn running_lines(conn: &Connection) -> Result<Vec<RunningLine>, AgentStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.status, m.id, m.content_blocks
         FROM proposals p JOIN messages m ON m.id = p.thread_line_id
         WHERE json_extract(m.content_blocks, '$.decision.outcome.kind') = 'running'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (group_id, status, message_id, content_blocks) = row?;
        let event: ConversationEvent = serde_json::from_str(&content_blocks).map_err(AgentStoreError::ContentBlocks)?;
        let ConversationEvent::ProposalDecided { decision } = event else {
            continue;
        };
        out.push(RunningLine {
            group_id,
            message_id,
            status: decode_token("status", status, ProposalStatus::from_token)?,
            decision,
        });
    }
    Ok(out)
}

/// Rewrite one line found by [`running_lines`]. No announcement: at launch no rail is showing it.
pub fn rewrite_line(conn: &Connection, message_id: i64, decision: ProposalDecision) -> Result<(), AgentStoreError> {
    rewrite(conn, message_id, decision).map(|_| ())
}

/// The group's status token and its line, or `None` for a group that doesn't exist.
fn pointer(conn: &Connection, group_id: i64) -> Result<Option<(String, Option<i64>)>, AgentStoreError> {
    Ok(conn
        .prepare_cached("SELECT status, thread_line_id FROM proposals WHERE id = ?1")?
        .query_row(params![group_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?)
}

fn append_line(
    tx: &Connection,
    group_id: i64,
    conversation_id: i64,
    decision: ProposalDecision,
    now: i64,
) -> Result<ThreadLine, AgentStoreError> {
    let content_blocks = event_json(decision)?;
    let (message_id, seq) = insert_message_row_in(
        tx,
        conversation_id,
        EVENT_ROLE_TOKEN,
        &content_blocks,
        "",
        None,
        None,
        now,
    )?;
    tx.prepare_cached("UPDATE proposals SET thread_line_id = ?2 WHERE id = ?1")?
        .execute(params![group_id, message_id])?;
    Ok(ThreadLine { message_id, seq })
}

/// Rewrite a line's decision in place, answering its `seq`, or `None` when the row is gone.
fn rewrite(conn: &Connection, message_id: i64, decision: ProposalDecision) -> Result<Option<i64>, AgentStoreError> {
    let content_blocks = event_json(decision)?;
    Ok(conn
        .prepare_cached("UPDATE messages SET content_blocks = ?2 WHERE id = ?1 AND role = ?3 RETURNING seq")?
        .query_row(params![message_id, content_blocks, EVENT_ROLE_TOKEN], |row| row.get(0))
        .optional()?)
}

/// A group's decision as its line says it: its verb, its display text, how many ops, and how it
/// went, plus, for a server group, the protocol its draft names.
///
/// The protocol is read best-effort: a draft that won't read (every reader fails it closed)
/// still leaves the line saying "the server `<name>`", because a lost word costs the agent less
/// than a lost lesson.
pub fn decision_of(
    conn: &Connection,
    group: &ProposalGroup,
    ops: u32,
    outcome: ProposalOutcomeKind,
) -> ProposalDecision {
    let protocol = if group.verb.is_server() {
        match read_server_proposal(conn, group.id) {
            Ok(proposal) => proposal.map(|proposal| proposal.draft.protocol),
            Err(e) => {
                log::warn!(target: "agent::store", "server group {}'s line names no protocol: {e}", group.id);
                None
            }
        }
    } else {
        None
    };
    ProposalDecision {
        verb: group.verb,
        what: group.display_name.clone(),
        ops,
        outcome,
        protocol,
    }
}

fn event_json(decision: ProposalDecision) -> Result<String, AgentStoreError> {
    serde_json::to_string(&ConversationEvent::ProposalDecided { decision }).map_err(AgentStoreError::ContentBlocks)
}
