//! What Synchronize folders sends over IPC: requests, typed refusals, pages of
//! grid rows, the confirm preview, the run report, and the five events. Shapes
//! only; the work is `session*.rs` and `cmdr-sync`.

use cmdr_sync::{
    Acknowledgement, Action, Category, ClockSkew, ContentProgress, ContentState, EntryKind, GuardBlock, GuardReport,
    KeepRefusal, OverrideRefusal, Place, PlanSummary, ProfileOverrides, ProfileProblem, RootConflict, RootRef,
    RunItemResult, RunProgress, Rung, ScanProgress, Side, SyncDefaults, SyncMaskError, SyncOptions,
};
use serde::{Deserialize, Serialize};

/// The rows of a pane's listing that "Only selected" compares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRef {
    /// The pane's listing.
    pub listing_id: String,
    /// The selected rows.
    pub indices: Vec<usize>,
    /// The listing state the indices were taken from.
    pub expected_sequence: u64,
}

/// One side of a compare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SideRequest {
    /// The folder.
    pub root: RootRef,
    /// The pane's selection, for "Only selected".
    pub selection: Option<SelectionRef>,
}

/// Everything a compare needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    /// The dialog's session, to compare again in; `None` opens a new one.
    pub session_id: Option<String>,
    /// The left folder.
    pub left: SideRequest,
    /// The right folder.
    pub right: SideRequest,
    /// The dialog's options.
    pub options: SyncOptions,
    /// The Settings values, read by the dialog.
    pub defaults: SyncDefaults,
    /// The profile's overrides (all `None` without a profile).
    pub overrides: ProfileOverrides,
    /// The profile's name, for `{profile}` in a custom versions folder.
    pub profile_name: Option<String>,
    /// Take hidden entries along.
    pub include_hidden: bool,
}

/// A compare that started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareStarted {
    /// Its session.
    pub session_id: String,
    /// Its generation: compare events of any other generation are stale.
    pub generation: u64,
}

/// Why a Synchronize folders command was refused. Typed, never a sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SyncError {
    /// No session has this id (closed, or never opened).
    NoSuchSession,
    /// The session has no finished compare yet.
    NotCompared,
    /// That volume isn't connected.
    VolumeNotConnected {
        /// Which side names it.
        side: Side,
        /// The id asked for.
        volume_id: String,
    },
    /// The root won't list: unmounted, gone, or refused.
    RootUnreachable {
        /// Which side.
        side: Side,
    },
    /// The root path climbs out of itself (`..`) or is empty.
    BadPath {
        /// Which side.
        side: Side,
    },
    /// The two roots are one folder, or one holds the other.
    RootsConflict {
        /// How.
        conflict: RootConflict,
    },
    /// The masks or the exclude list don't parse.
    BadMask {
        /// What and where.
        error: SyncMaskError,
    },
    /// A Settings or profile value is out of range.
    BadSettings {
        /// Which.
        problem: ProfileProblem,
    },
    /// A custom versions folder on this Mac didn't answer, so it can't be
    /// shown to lie outside the synchronized folders.
    KeepFolderUnreadable,
    /// The versions folder can't be used for these roots.
    Keep {
        /// Why.
        refusal: KeepRefusal,
    },
    /// The mode writes to a side whose volume is read-only.
    SideReadOnly {
        /// Which side.
        side: Side,
    },
    /// The pane's selection moved on since the dialog read it.
    StaleSelection {
        /// Which side.
        side: Side,
    },
    /// A compare is still running in this session.
    CompareRunning,
    /// A run is going on in this session, or for one of these folders, one
    /// inside them, or one around them.
    RunInProgress,
    /// The plan or the Settings changed since the confirm the person saw:
    /// show the confirm again.
    PlanChanged,
    /// A run has changed the folders since this compare: compare again.
    StaleCompare,
    /// An action change was refused.
    Override {
        /// Why.
        refusal: OverrideRefusal,
    },
    /// That row can't be compared by content (not two files of one size, or a
    /// volume that can't be read).
    NotContentComparable {
        /// The row.
        item: u32,
    },
    /// The write engine isn't ready yet (only before the app has started).
    NotInstalled,
}

/// One grid row: a folder header, or a plan row with both sides' facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ViewRowWire {
    /// "folder `ordinal` of `of`".
    Folder {
        /// The folder's path from the root, in the left spelling when there is one.
        rel_dir: String,
        /// Its position among the visible headers, from 1.
        ordinal: u32,
        /// How many headers are visible.
        of: u32,
    },
    /// A plan row.
    Item {
        /// Its index in the plan.
        index: u32,
        /// The left name, in its own spelling.
        left_name: Option<String>,
        /// The right name, in its own spelling.
        right_name: Option<String>,
        /// The left path from the root, in the left's own spelling (F3 and F4 open it).
        left_rel_path: Option<String>,
        /// The right path from the root, in the right's own spelling.
        right_rel_path: Option<String>,
        /// What the left entry is.
        left_kind: Option<EntryKind>,
        /// What the right entry is.
        right_kind: Option<EntryKind>,
        /// File size, or a folder's total.
        left_size: Option<u64>,
        /// File size, or a folder's total.
        right_size: Option<u64>,
        /// Seconds since the epoch.
        left_mtime: Option<u64>,
        /// Seconds since the epoch.
        right_mtime: Option<u64>,
        /// How the sides relate.
        category: Category,
        /// What settled it.
        rung: Rung,
        /// The byte compare's answer, if one was asked for.
        content: ContentState,
        /// Both sizes known and equal.
        same_size: bool,
        /// What the run does with the row.
        action: Action,
        /// What the mode chose (`action != default_action` is "changed by you").
        default_action: Action,
        /// Replaces a newer file with an older one.
        older_overwrites_newer: bool,
        /// The actions this row may take.
        legal: Vec<Action>,
    },
}

/// A grid row with its place in the filtered view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncRowWire {
    /// Its position in the view.
    pub view_index: u32,
    /// The row.
    pub row: ViewRowWire,
}

/// A window of grid rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncRowsPage {
    /// Rows in the whole view.
    pub total: u32,
    /// The rows asked for.
    pub rows: Vec<SyncRowWire>,
}

/// The summary bar and the guards, after a compare or a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    /// Counts for the summary bar.
    pub summary: PlanSummary,
    /// What the guards say about the current plan.
    pub guards: GuardReport,
    /// A run has changed the folders since this compare: compare again.
    pub stale: bool,
    /// The plan revision this state is of. Revisions only grow, across
    /// sessions too, so an answer with a lower one than the dialog holds is late.
    pub revision: u64,
}

/// What a bulk key did: the state after, and every selected row it left alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncActionsOutcome {
    /// The summary and guards after the change.
    pub state: SyncState,
    /// Rows the key couldn't change, grouped by why.
    pub skipped: Vec<cmdr_sync::SkippedRows>,
    /// Selected rows the show-filters hide, which a key never changes.
    pub hidden: u32,
}

/// A group of the confirm, each with its own checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ConfirmGroupKey {
    /// New items copied left to right.
    CopyRight,
    /// New items copied right to left.
    CopyLeft,
    /// Copies to the left of items the mode would have removed on the right.
    CopyLeftRecreates,
    /// Copies to the right of items the mode would have removed on the left.
    CopyRightRecreates,
    /// Files on the right replaced by the left's.
    ReplaceRight,
    /// Files on the left replaced by the right's.
    ReplaceLeft,
    /// Items removed on the right.
    RemoveRight,
    /// Items removed on the left.
    RemoveLeft,
}

/// Where replaced or removed items of a group go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum KeepDestinationWire {
    /// The volume's Trash.
    Trash,
    /// This run's dated versions folder.
    Folder {
        /// The volume it is on.
        volume_id: String,
        /// The folder.
        path: String,
    },
}

/// A row the confirm names: one that replaces a newer file with an older one,
/// or a `?` row the person gave an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WarnedItem {
    /// The row.
    pub item: u32,
    /// Its path from the root.
    pub rel_path: String,
    /// Seconds since the epoch.
    pub left_mtime: Option<u64>,
    /// Seconds since the epoch.
    pub right_mtime: Option<u64>,
}

/// One group of the confirm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmGroup {
    /// Which.
    pub key: ConfirmGroupKey,
    /// Files it copies or removes.
    pub files: u64,
    /// Their bytes.
    pub bytes: u64,
    /// Where what it replaces or removes goes; `None` for new copies.
    pub keep: Option<KeepDestinationWire>,
}

/// What the confirm shows before a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmPreview {
    /// The plan revision this preview shows; the run must name it.
    pub revision: u64,
    /// The groups, in a fixed order.
    pub groups: Vec<ConfirmGroup>,
    /// The first rows that replace a newer file with an older one.
    pub older_overwrites_newer: Vec<WarnedItem>,
    /// How many such rows there are in all.
    pub older_overwrites_newer_total: u64,
    /// Guards that hold actions back.
    pub blocks: Vec<GuardBlock>,
    /// A uniform clock offset, if one shows.
    pub skew: Option<ClockSkew>,
    /// Actions that run on rows the show-filters hide.
    pub hidden_actions: u64,
    /// The first `?` rows the person gave an action anyway (the compare
    /// couldn't tell which side is newer, so only that choice decides).
    pub chosen_unknown: Vec<WarnedItem>,
    /// How many such rows there are in all.
    pub chosen_unknown_total: u64,
    /// Kept items go to another volume (a copy, then a removal).
    pub keep_is_slow: bool,
}

/// The confirm's accept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    /// The revision of the confirm the person accepted.
    pub revision: u64,
    /// The groups left ticked.
    pub groups: Vec<ConfirmGroupKey>,
    /// The second yes for each guard the person released.
    pub acknowledged: Vec<Acknowledgement>,
    /// The Settings values the confirm was shown with (any other is `PlanChanged`).
    pub defaults: SyncDefaults,
}

/// How a compare ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CompareOutcome {
    /// The rows are ready.
    Done,
    /// Cancelled, or replaced by a newer compare.
    Stopped,
    /// It couldn't run.
    Refused {
        /// Why.
        error: SyncError,
    },
}

/// One row's end in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunItemWire {
    /// The row.
    pub item: u32,
    /// How it ended.
    pub result: RunItemResult,
}

/// An accepted row a guard held back at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HeldRowWire {
    /// The row.
    pub item: u32,
    /// The guard that holds it.
    pub block: GuardBlock,
}

/// A folder a run made for copies that didn't land.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EmptyFolderWire {
    /// The side it is on.
    pub side: Side,
    /// The folder.
    pub path: String,
}

/// A dated folder a run made and couldn't remove again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverFolderWire {
    /// Which volume it is on: the side's, or the local disk (a custom versions folder).
    pub place: Place,
    /// The side whose items it was made for.
    pub side: Side,
    /// The folder.
    pub path: String,
}

/// How a run ended, for the result screen.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunReportWire {
    /// Rows done.
    pub done: u64,
    /// Rows left alone because something changed since compare.
    pub changed_since_compare: u64,
    /// Rows left alone because the volume has no Trash (Trash only).
    pub no_trash: u64,
    /// Rows the engine couldn't do, or whose keep folder couldn't be used.
    pub could_not_do: u64,
    /// Old files kept whose new copy didn't land: the place is empty.
    pub kept_not_replaced: u64,
    /// Rows held by a guard (`held_by_guard`), plus removals held because a copy
    /// beside them didn't land (`HeldAfterFailedCopy` in `items`).
    pub held: u64,
    /// Rows the run never reached.
    pub not_started: u64,
    /// Versions folders that received something.
    pub versions_folders: Vec<KeepDestinationWire>,
    /// Bytes put in versions folders.
    pub kept_bytes: u64,
    /// The engine operations the run started (the queue window and undo know them).
    pub operation_ids: Vec<String>,
    /// Every row the run reached and its end, for "Show in grid".
    pub items: Vec<RunItemWire>,
    /// Accepted rows a guard held back (copies and replacements too, not only
    /// removals), each with the guard: they never reached the run.
    pub held_by_guard: Vec<HeldRowWire>,
    /// Target folders the run made for copies that then didn't land: empty,
    /// and they stay (the run never removes anything).
    pub empty_folders: Vec<EmptyFolderWire>,
    /// Dated folders the run made, kept nothing in, and couldn't remove again:
    /// they stay.
    pub leftover_folders: Vec<LeftoverFolderWire>,
    /// Cancelled, or the engine stopped.
    pub stopped: bool,
}

/// Live counts while a compare runs (at most every 100 ms).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "sync-compare-progress")]
pub struct SyncCompareProgress {
    /// The session.
    pub session_id: String,
    /// Which compare of the session.
    pub generation: u64,
    /// The left scan.
    pub left: ScanProgress,
    /// The right scan.
    pub right: ScanProgress,
    /// The byte compare, once it runs.
    pub content: Option<ContentProgress>,
}

/// A compare ended.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "sync-compare-finished")]
pub struct SyncCompareFinished {
    /// The session.
    pub session_id: String,
    /// Which compare of the session.
    pub generation: u64,
    /// How.
    pub outcome: CompareOutcome,
}

/// Rows changed without a new compare (a row's byte compare answered).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "sync-rows-changed")]
pub struct SyncRowsChanged {
    /// The session.
    pub session_id: String,
}

/// A run's totals after a step.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "sync-run-progress")]
pub struct SyncRunProgress {
    /// The session.
    pub session_id: String,
    /// Steps, rows, and bytes so far.
    pub progress: RunProgress,
}

/// A run ended.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "sync-run-finished")]
pub struct SyncRunFinished {
    /// The session.
    pub session_id: String,
    /// What happened.
    pub report: RunReportWire,
}
