//! The `important_folders` and `folder_importance` agent tools.
//!
//! Both REUSE the shipped `cmdr://importance` snapshot core
//! (`mcp::resources::importance::snapshot_*`), which already handles the headline
//! behaviors: answering OFFLINE for an unmounted-but-scored drive (the importance
//! stores outlive the mount) and the floored-vs-unscored derivation. This file
//! places each call on volumes through `crate::location`'s resolver, translates
//! between app paths and each store's own path space at one seam
//! ([`store_path`] / [`app_location`]), shapes the typed snapshots into the
//! agent's JSON, and adds the staleness comparison (`asOfGeneration` vs the
//! volume's current `recomputeGeneration`, spec §2.4).

use std::path::Path;

use cmdr_fs::volume::app_paths::rebase;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::location::Location;
use crate::location::miss::LocationMissResult;
use crate::location::resolve::{LocationInput, LocationMiss, VolumeFacts, resolve};
use crate::mcp::resources::importance::{
    PathImportance, RankedFolder, RankedScope, VolumeOverview, snapshot_overview, snapshot_path_on, snapshot_threshold,
    snapshot_top,
};
use crate::mcp::{ToolError, ToolResult};
use cmdr_index::importance::{Explanation, FloorReason, ImportanceIndex, SignalSet};

const DEFAULT_TOP_N: usize = 20;

// ── The path-space seam ───────────────────────────────────────────────────────

/// `app_path` in its volume's importance store's path space: the store keeps its
/// index's paths, volume-relative (`/papers` for `/Volumes/naspi-1/papers`) and
/// absolute only on the boot disk, whose root is `/`. A cloud drive's store is
/// relative to the DRIVE's root, though `root`'s index walks it, so this always
/// takes the importance volume's own root, ❌ never the index's routing.
fn store_path(app_path: &str, root: &str) -> Option<String> {
    rebase(app_path, root, "/")
}

/// A store row's place on the wire: its path joined onto the volume's root (from
/// the one root rule, so an offline share answers under its last root).
fn app_location(volume_id: &str, store_path: &str, root: &str) -> Option<Location> {
    rebase(store_path, "/", root).map(|path| Location {
        volume_id: volume_id.to_string(),
        path,
    })
}

fn to_value(result: &impl Serialize) -> ToolResult {
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
}

// ── important_folders ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedFolderOut {
    pub location: Location,
    /// Rounded to three decimals: more digits are noise and cost tokens.
    pub score: f64,
    /// The row's strongest signals, like `projectRoot, changed today, 6 visits`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub why: String,
    /// Near-duplicate descendants folded into this row; pass its location as
    /// `under` + `volumeId` to list them.
    #[serde(skip_serializing_if = "is_zero")]
    pub folded: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeOverviewOut {
    pub volume_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The volume's current recompute generation (its scored-ness age marker).
    pub generation: u64,
    pub folder_count: u64,
    /// Scored under an id no volume the app knows carries (a share once reached by
    /// another address), so its rows have no root to stand on and are left out.
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub unplaced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportantFoldersResult {
    /// The `under` folder the ranking was scoped to, as resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<Location>,
    pub folders: Vec<RankedFolderOut>,
    /// `true` when more folders matched than were returned (threshold mode).
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub truncated: bool,
    /// The scored volumes and their current generations, so staleness is visible.
    pub volumes: Vec<VolumeOverviewOut>,
}

/// Shape a ranked list plus the volume overview, placing each row on its volume's
/// root. A row whose volume `facts` can't place is left out (the read already
/// skips those volumes; this keeps an invented root unrepresentable). Pure.
pub(crate) fn build_important_folders(
    folders: Vec<RankedFolder>,
    truncated: bool,
    overview: Vec<VolumeOverview>,
    facts: &VolumeFacts,
    under: Option<Location>,
) -> ImportantFoldersResult {
    ImportantFoldersResult {
        under,
        folders: folders
            .into_iter()
            .filter_map(|f| {
                let root = &facts.get(&f.volume_id)?.root;
                Some(RankedFolderOut {
                    location: app_location(&f.volume_id, &f.path, root)?,
                    score: (f.score * 1000.0).round() / 1000.0,
                    why: f.why,
                    folded: f.folded,
                })
            })
            .collect(),
        truncated,
        volumes: overview
            .into_iter()
            .map(|v| VolumeOverviewOut {
                unplaced: facts.get(&v.volume_id).is_none(),
                volume_id: v.volume_id,
                kind: v.kind.map(|k| k.to_string()),
                generation: v.generation,
                folder_count: v.folder_count,
            })
            .collect(),
    }
}

/// A ranked read's scope, placed: one volume (and maybe a folder on it), or every
/// volume the facts can place.
struct PlacedScope {
    volume_id: Option<String>,
    /// The `under` folder as an app location, echoed in the result.
    under: Option<Location>,
    /// The same folder in the store's path space, what the read filters by.
    under_in_store: Option<String>,
    placed: Vec<String>,
}

impl PlacedScope {
    fn ranked(&self) -> RankedScope<'_> {
        RankedScope {
            volume: self.volume_id.as_deref(),
            under: self.under_in_store.as_deref(),
            among: Some(&self.placed),
        }
    }
}

/// Place `volumeId` + `under` through the resolver. A bare `under` scopes the read
/// to the one live volume that holds it.
fn place_scope(facts: &VolumeFacts, volume_id: Option<&str>, under: Option<&str>) -> Result<PlacedScope, LocationMiss> {
    let placed = facts.volumes.iter().map(|v| v.id.clone()).collect();
    let Some(under) = under else {
        if let Some(id) = volume_id
            && facts.get(id).is_none()
        {
            // No folder was asked about, so the miss echoes an empty path.
            return Err(LocationMiss::UnknownVolume {
                requested: LocationInput {
                    volume_id: Some(id.to_string()),
                    path: String::new(),
                },
            });
        }
        return Ok(PlacedScope {
            volume_id: volume_id.map(str::to_string),
            under: None,
            under_in_store: None,
            placed,
        });
    };
    let resolved = resolve(
        facts,
        &LocationInput {
            volume_id: volume_id.map(str::to_string),
            path: under.to_string(),
        },
    )?;
    Ok(PlacedScope {
        under_in_store: store_path(&resolved.location.path, &resolved.root),
        volume_id: Some(resolved.location.volume_id.clone()),
        under: Some(resolved.location),
        placed,
    })
}

pub fn important_folders_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "top": { "type": "integer", "description": "Default 20; ignored when threshold is set." },
            "threshold": { "type": "number", "description": "Every folder scoring at or above this (0-1) instead of a top-N." },
            "volumeId": { "type": "string", "description": "One volume, from list_volumes or a row's location; omit for all scored volumes." },
            "under": { "type": "string", "description": "Only folders at or below this folder (on volumeId when given); ~ expands." }
        },
        "additionalProperties": false
    })
}

pub async fn execute_important_folders<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    important_folders_on(&data_dir, &VolumeFacts::now().await, params, now_secs())
}

/// `important_folders` over a data dir and the volume facts, so the whole call is
/// testable without an app.
pub(crate) fn important_folders_on(data_dir: &Path, facts: &VolumeFacts, params: &Value, now: u64) -> ToolResult {
    let str_param = |key: &str| params.get(key).and_then(Value::as_str).filter(|s| !s.is_empty());
    let scope = match place_scope(facts, str_param("volumeId"), str_param("under")) {
        Ok(scope) => scope,
        Err(miss) => return to_value(&LocationMissResult::from(miss)),
    };
    let overview = snapshot_overview(data_dir);

    let (folders, truncated) = if let Some(threshold) = params.get("threshold").and_then(Value::as_f64) {
        snapshot_threshold(data_dir, threshold, scope.ranked(), now).map_err(ToolError::invalid_params)?
    } else {
        let n = params
            .get("top")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_TOP_N);
        let folders = snapshot_top(data_dir, n, scope.ranked(), now).map_err(ToolError::invalid_params)?;
        (folders, false)
    };
    to_value(&build_important_folders(
        folders,
        truncated,
        overview,
        facts,
        scope.under,
    ))
}

// ── folder_importance ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum FolderImportanceResult {
    /// The folder has a stored weight, with its breakdown and staleness.
    Scored {
        location: Location,
        score: f64,
        as_of_generation: u64,
        recompute_generation: u64,
        /// `true` when the stored score predates the volume's latest recompute.
        stale: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        signals: Option<Explanation>,
    },
    /// The folder floors to zero by design, with why.
    Floored {
        location: Location,
        reason: String,
        note: String,
    },
    /// The folder isn't scored and doesn't floor.
    Unscored { location: Location, note: String },
}

/// The Scored result, deriving staleness from the two generations. Pure.
pub(crate) fn scored_result(
    location: Location,
    score: f64,
    as_of_generation: u64,
    recompute_generation: u64,
    signals: Option<Explanation>,
) -> FolderImportanceResult {
    FolderImportanceResult::Scored {
        location,
        score,
        as_of_generation,
        recompute_generation,
        stale: as_of_generation < recompute_generation,
        signals,
    }
}

/// The Floored result, labeling the reason (mirrors `cmdr://importance`'s labels).
pub(crate) fn floored_result(location: Location, reason: FloorReason) -> FolderImportanceResult {
    let (label, note) = match reason {
        FloorReason::NameDenylisted => (
            "nameDenylisted",
            "Floored to zero by design: its name is denylisted (build output, cache, or VCS internals like node_modules or .git).",
        ),
        FloorReason::HiddenOrSystem => (
            "hiddenOrSystem",
            "Floored to zero by design: it's hidden (dot-prefixed) or system-owned.",
        ),
        FloorReason::UnderFlooredAncestor => (
            "underFlooredAncestor",
            "Floored to zero by design: a denylisted, hidden, or system ancestor floors its whole subtree.",
        ),
    };
    FolderImportanceResult::Floored {
        location,
        reason: label.to_string(),
        note: note.to_string(),
    }
}

pub(crate) fn unscored_result(location: Location) -> FolderImportanceResult {
    FolderImportanceResult::Unscored {
        location,
        note: "No stored weight — either this volume hasn't been scored yet, or the folder isn't in the index."
            .to_string(),
    }
}

pub fn folder_importance_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute or ~-relative folder to explain." },
            "volumeId": { "type": "string", "description": "The volume the path is on, from list_volumes or a result's location. Omit for a path on a connected volume." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

pub async fn execute_folder_importance<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let input =
        LocationInput::from_params(params).ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;
    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    folder_importance_on(&data_dir, &VolumeFacts::now().await, &input, now_secs())
}

/// `folder_importance` over a data dir and the volume facts: the resolver picks ONE
/// volume, and only that volume's store answers.
pub(crate) fn folder_importance_on(
    data_dir: &Path,
    facts: &VolumeFacts,
    input: &LocationInput,
    now: u64,
) -> ToolResult {
    let resolved = match resolve(facts, input) {
        Ok(resolved) => resolved,
        Err(miss) => return to_value(&LocationMissResult::from(miss)),
    };
    let volume_id = resolved.location.volume_id.as_str();
    let in_store = store_path(&resolved.location.path, &resolved.root)
        .ok_or_else(|| ToolError::internal("the resolver placed a path outside its volume's root"))?;

    let result = match snapshot_path_on(data_dir, volume_id, &in_store, now) {
        PathImportance::Scored {
            weight, explanation, ..
        } => {
            // The volume's CURRENT generation, to flag a stale stored score. The
            // availability mask doesn't affect the generation read, so `all()` is fine.
            let recompute = ImportanceIndex::open(data_dir, volume_id, SignalSet::all())
                .recompute_generation()
                .unwrap_or(0);
            scored_result(
                resolved.location,
                weight.score.value(),
                weight.as_of_generation,
                recompute,
                explanation,
            )
        }
        PathImportance::Floored { reason } => floored_result(resolved.location, reason),
        PathImportance::Unscored => unscored_result(resolved.location),
    };
    to_value(&result)
}

/// Wall-clock unix seconds for the `explain` recency terms. A tool call reads the
/// current time directly; threading the envelope's clock through the runtime (so a
/// tool call and its explanation agree on "now") is a refinement, not yet wired.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "importance_tests.rs"]
mod tests;
