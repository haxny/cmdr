//! What a group's files are like on disk NOW, a window at a time, beside the creation snapshot
//! `suggested_ops_page` carries. Its own file because it's the one suggested-ops read that
//! touches the source volume per row; the deadline and the shared stat budget live in
//! `agent/suggested_ops/facts.rs`.

use serde::Serialize;
use tauri::AppHandle;

use super::suggested_ops::op_window;
use crate::agent::store::proposals::{get_group, page_ops};
use crate::agent::suggested_ops::facts::{SourceStat, stat_deadline, stat_sources};

/// What one op's source is like on disk right now, beside its creation snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedOpLiveFact {
    pub op_id: i64,
    pub state: LiveFileState,
}

/// One source as its volume answers for it now. Three answers the dialog words differently,
/// so none collapses into another: a missing file is a fact, an unanswered stat is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LiveFileState {
    Present {
        /// Bytes. `None` for a folder, whose own stat size means nothing to a user.
        size: Option<u64>,
        /// Unix seconds.
        modified: Option<i64>,
    },
    /// The volume says nothing is at that path any more.
    Missing,
    /// The volume didn't answer within the deadline, or refused the stat.
    Unknown,
}

/// Why a live-facts read didn't happen at all. Per-file trouble is never an error: it's that
/// file's `Unknown`.
///
/// ❌ Not prose: the dialog words each variant itself; the log records the detail.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SuggestedOpsLiveFactsError {
    /// The suggestion store isn't open: agent features are off, or the app is still starting.
    StoreNotOpen,
    /// The store refused the read.
    Store {
        /// What SQLite reported, for the log.
        detail: String,
    },
    /// No group with that id, which usually means the list on screen is stale.
    UnknownGroup,
    /// The group's source volume is one Cmdr knows (a saved share or server, a listed phone)
    /// but nothing has connected it, so there's nothing to ask.
    SourceNotConnected,
    /// No volume is registered under the group's source volume id any more (a drive that left).
    SourceGone,
    /// Nothing above classifies it (a panicked task).
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

/// The current size and date of one window of a group's ops, so the dialog can draw a page
/// from the frozen rows first and fill in what's true now as it arrives.
///
/// Every stat goes through the group's SOURCE volume, under one shared deadline per call
/// (`io_budget`: 2 s, at least 10 s on a live session), with the stats capped across every
/// caller by `facts.rs`'s `BlockingBudget`: the dialog re-asks on scroll faster than a slow
/// share answers.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_live_facts(
    app: AppHandle,
    group_id: i64,
    offset: u32,
    limit: u32,
) -> Result<Vec<SuggestedOpLiveFact>, SuggestedOpsLiveFactsError> {
    // The same cap as the page: a window is stat'ed whole, so an unbounded one would be an
    // unbounded wait on a slow share.
    let limit = op_window(limit);
    let db_path = super::db_path(&app).ok_or(SuggestedOpsLiveFactsError::StoreNotOpen)?;
    let (group, ops) = tauri::async_runtime::spawn_blocking(move || {
        let read = || -> Result<_, crate::agent::store::AgentStoreError> {
            let conn = crate::agent::store::open_read_connection(&db_path)?;
            let Some(group) = get_group(&conn, group_id)? else {
                return Ok(None);
            };
            Ok(Some((group, page_ops(&conn, group_id, limit, offset)?)))
        };
        read()
    })
    .await
    .map_err(|e| SuggestedOpsLiveFactsError::Unexpected { detail: e.to_string() })?
    .map_err(|e| SuggestedOpsLiveFactsError::Store { detail: e.to_string() })?
    .ok_or(SuggestedOpsLiveFactsError::UnknownGroup)?;

    let volume_id = group.source_volume_id;
    let Some(volume) = crate::file_system::volume::manager::get_volume_manager().get(&volume_id) else {
        return Err(match crate::unregistered_volumes::why_unregistered(&volume_id).await {
            crate::unregistered_volumes::Unregistered::NotConnected => SuggestedOpsLiveFactsError::SourceNotConnected,
            crate::unregistered_volumes::Unregistered::Gone => SuggestedOpsLiveFactsError::SourceGone,
        });
    };
    let deadline = stat_deadline(volume.as_ref());
    let paths = ops.iter().map(|op| op.source_path.clone()).collect();
    let stats = stat_sources(volume, paths, &deadline).await;
    Ok(ops
        .into_iter()
        .zip(stats)
        .map(|(op, stat)| SuggestedOpLiveFact {
            op_id: op.id,
            state: live_state(stat),
        })
        .collect())
}

fn live_state(stat: SourceStat) -> LiveFileState {
    match stat {
        SourceStat::Present { size, modified, .. } => LiveFileState::Present { size, modified },
        SourceStat::Missing => LiveFileState::Missing,
        SourceStat::Unknown => LiveFileState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A live fact crosses the wire as a tagged union the dialog can switch on, and a folder's or
    /// an undated file's missing number stays `null`, never a zero.
    #[test]
    fn a_live_fact_keeps_its_three_answers_apart_on_the_wire() {
        let present = live_state(SourceStat::Present {
            size: Some(7),
            modified: Some(1_700_000_000),
            inode: Some(12),
        });
        assert_eq!(
            serde_json::to_value(SuggestedOpLiveFact {
                op_id: 4,
                state: present
            })
            .unwrap(),
            serde_json::json!({ "opId": 4, "state": { "kind": "present", "size": 7, "modified": 1_700_000_000 } })
        );
        let folder = live_state(SourceStat::Present {
            size: None,
            modified: None,
            inode: None,
        });
        assert_eq!(
            serde_json::to_value(folder).unwrap(),
            serde_json::json!({ "kind": "present", "size": null, "modified": null })
        );
        assert_eq!(
            serde_json::to_value(live_state(SourceStat::Missing)).unwrap(),
            serde_json::json!({ "kind": "missing" })
        );
        assert_eq!(
            serde_json::to_value(live_state(SourceStat::Unknown)).unwrap(),
            serde_json::json!({ "kind": "unknown" })
        );
    }

    /// A source volume nobody connected and one that left call for different words, so they stay
    /// two variants.
    #[test]
    fn a_live_facts_refusal_names_why_the_source_cant_be_asked() {
        assert_eq!(
            serde_json::to_value(SuggestedOpsLiveFactsError::SourceNotConnected).unwrap(),
            serde_json::json!({ "type": "sourceNotConnected" })
        );
        assert_eq!(
            serde_json::to_value(SuggestedOpsLiveFactsError::SourceGone).unwrap(),
            serde_json::json!({ "type": "sourceGone" })
        );
        assert_eq!(
            serde_json::to_value(SuggestedOpsLiveFactsError::UnknownGroup).unwrap(),
            serde_json::json!({ "type": "unknownGroup" })
        );
    }
}
