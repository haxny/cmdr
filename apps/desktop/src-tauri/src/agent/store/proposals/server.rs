//! A server group's writes and its claim: the draft written beside the group, the claim that
//! approves it, and its completion.
//!
//! The draft sits in `proposal_server_drafts` (v13), written in the group's OWN transaction
//! (`write.rs`), so a server group without one can't be committed, and every read of it fails
//! closed (`read.rs`). A server group takes its own claim ([`claim_server_group`]): the file
//! claim's acceptance record binds ops, and a server group has none, so the REVISION is its
//! binding. Why: `DETAILS.md` § A server group.

use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use super::super::AgentStoreError;
use super::draft::{ServerDraft, ServerEdit};
use super::read::{get_group, read_sidecar};
use super::{CompleteOutcome, ProposalGroup, mark_group_completed};
use crate::agent::types::{ProposalStatus, ServerSaveNote};

/// A server group the claim moved into `approved`, with what to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedServer {
    pub group: ProposalGroup,
    pub draft: ServerDraft,
    pub edit: Option<ServerEdit>,
}

/// What a server claim did, or why it didn't.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "one per click, moved straight into the save: boxing buys nothing"
)]
pub enum ServerClaimOutcome {
    Claimed(ClaimedServer),
    /// The group was amended since the card the user approved from. Nothing changed.
    RevisionChanged {
        shown: i64,
        found: i64,
    },
    /// It already left `pending`.
    NotPending {
        found: ProposalStatus,
    },
    /// A file group: its claim binds ops through the acceptance record, never this one.
    NotAServerGroup,
    /// No group with that id.
    Unknown,
}

/// Write `group_id`'s draft row, replacing any. Inside the caller's transaction: `create_group`
/// and `repropose_group` call it so the draft and the group header commit together.
pub(super) fn write_server_draft(
    tx: &Transaction<'_>,
    group_id: i64,
    draft: &ServerDraft,
    edit: Option<&ServerEdit>,
) -> Result<(), AgentStoreError> {
    tx.prepare_cached(
        "INSERT INTO proposal_server_drafts (group_id, draft, editing_id, previous, completion_note)
         VALUES (?1, ?2, ?3, ?4, NULL)
         ON CONFLICT(group_id) DO UPDATE SET
            draft = excluded.draft, editing_id = excluded.editing_id, previous = excluded.previous,
            completion_note = NULL",
    )?
    .execute(params![
        group_id,
        to_json(draft),
        edit.map(|edit| edit.editing.as_str()),
        edit.map(|edit| to_json(&edit.previous))
    ])?;
    Ok(())
}

/// Drop `group_id`'s draft row, for a group whose intent isn't a server's (an amend that turns
/// a server group into a file group leaves no stale draft behind).
pub(super) fn delete_server_draft(tx: &Transaction<'_>, group_id: i64) -> Result<(), AgentStoreError> {
    tx.prepare_cached("DELETE FROM proposal_server_drafts WHERE group_id = ?1")?
        .execute(params![group_id])?;
    Ok(())
}

/// Claim a server group for saving: the server half of the claim transaction.
///
/// ONE `BEGIN IMMEDIATE`: read the group, refuse a file group, a stale status, or a revision
/// other than `shown_revision`, read the draft, then `UPDATE ... WHERE status = 'pending' AND
/// revision = ?`. The revision IS the binding: a draft changes only through `repropose_group`,
/// which bumps it, and nothing changes it after the claim. So what comes back is exactly the
/// card the user read, or a refusal.
///
/// A draft that won't decode fails the whole claim (the transaction rolls back), so a group
/// nothing can show is never `approved` with nothing to save.
pub fn claim_server_group(
    conn: &Connection,
    group_id: i64,
    shown_revision: i64,
    now: i64,
) -> Result<ServerClaimOutcome, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let Some(group) = get_group(&tx, group_id)? else {
        return Ok(ServerClaimOutcome::Unknown);
    };
    if !group.verb.is_server() {
        return Ok(ServerClaimOutcome::NotAServerGroup);
    }
    if group.status != ProposalStatus::Pending {
        return Ok(ServerClaimOutcome::NotPending { found: group.status });
    }
    if group.revision != shown_revision {
        return Ok(ServerClaimOutcome::RevisionChanged {
            shown: shown_revision,
            found: group.revision,
        });
    }
    let proposal = read_sidecar(&tx, group_id)?;
    // Conditional on both, though the reads above already hold them under this lock: the
    // statement is the claim, so it says what it claims.
    let updated = tx
        .prepare_cached(
            "UPDATE proposals SET status = ?3, decided_at = ?5
             WHERE id = ?1 AND status = ?2 AND revision = ?4",
        )?
        .execute(params![
            group_id,
            ProposalStatus::Pending.as_token(),
            ProposalStatus::Approved.as_token(),
            shown_revision,
            now
        ])?;
    if updated == 0 {
        return Ok(ServerClaimOutcome::NotPending { found: group.status });
    }
    let group = get_group(&tx, group_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    tx.commit()?;
    Ok(ServerClaimOutcome::Claimed(ClaimedServer {
        group,
        draft: proposal.draft,
        edit: proposal.edit,
    }))
}

/// Mark a claimed server group completed, with what its save left undone when it was partial.
///
/// The same conditional `approved` → `completed` as a file group's ([`mark_group_completed`]),
/// and the note lands in the same transaction, so a group never reads completed without the
/// note its card, thread line, and the agent all read.
pub fn mark_server_group_completed(
    conn: &Connection,
    group_id: i64,
    note: Option<ServerSaveNote>,
) -> Result<CompleteOutcome, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let outcome = mark_group_completed(&tx, group_id)?;
    if outcome == CompleteOutcome::Completed {
        tx.prepare_cached("UPDATE proposal_server_drafts SET completion_note = ?2 WHERE group_id = ?1")?
            .execute(params![group_id, note.map(ServerSaveNote::as_token)])?;
    }
    tx.commit()?;
    Ok(outcome)
}

fn to_json(draft: &ServerDraft) -> String {
    serde_json::to_string(draft).expect("a ServerDraft is plain strings and numbers, so it always serializes")
}
