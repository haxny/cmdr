// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]
#![deny(missing_docs)]

//! Synchronize folders, the pure part: what a volume can do, how names pair up
//! across volumes, and the one answer to "is this copy newer?" that Compare
//! folders (⇧F2) and Synchronize folders share.
//!
//! No `tauri`, no I/O. Rationale and tables: `DETAILS.md`.

mod caps;
mod compare;
mod content;
mod context;
mod control;
mod execute;
mod filter;
mod guards;
mod keep;
mod key;
mod overrides;
mod plan;
mod profile;
mod runner;
mod scan;
mod snapshot;
mod steps;
mod view;

pub use caps::{BackendFacts, SyncCaps, backend_facts};
pub use compare::{
    Category, CompareBy, CompareRules, ContentState, EntryFacts, EntryKind, HOUR_SHIFT_SECS, Rung,
    SAME_TIME_TOLERANCE_SECS, Verdict, classify,
};
pub use content::{ContentJob, ContentProgress, compare_contents, content_workers};
pub use context::RunContext;
pub use control::SyncControl;
pub use execute::{
    BatchOutcome, BoundSource, EngineBatch, EngineFuture, ItemOutcome, Observed, RunItemResult, RunProgress, RunReport,
    SyncEngine, execute,
};
pub use filter::{ALWAYS_EXCLUDED, Admission, DEFAULT_EXCLUDES, JUNK_MASKS, MaskFilter, SyncMaskError};
pub use guards::{
    Acknowledgement, ClockSkew, DEFAULT_MAX_REMOVE_PERCENT, Gated, GuardBlock, GuardReport, GuardSettings, check, gate,
};
pub use keep::{
    DEFAULT_VERSIONS_FOLDER, KeepDestination, KeepPlace, KeepPolicy, KeepRefusal, Place, RunStamp, VersionsLocation,
    VersionsRoot, keep_destination, kept_dir, staging_root, versions_root,
};
pub use key::{EntryKey, NameRule};
pub use overrides::{ActionChange, BulkAction, OverrideRefusal, apply_overrides, bulk_changes, legal_actions};
pub use plan::{
    Action, Baseline, HeadFolder, Mode, Plan, PlanItem, PlanOptions, Reclassified, content_jobs, default_action, plan,
    reclassify,
};
pub use profile::{
    Effective, MAX_REMOVE_COUNT, MAX_REMOVE_PERCENT, MAX_TOLERANCE_SECS, ProfileOverrides, ProfileProblem,
    RootConflict, RootRef, SyncDefaults, SyncOptions, SyncProfile, check_keep_policy, effective, roots_conflict,
    validate, validate_defaults,
};
pub use scan::{ScanError, ScanOptions, ScanProgress, Side, scan, subtree_now};
pub use snapshot::{Entry, Snapshot, SubtreeFacts, UnreadableReason};
pub use steps::{Step, StepPlan, steps};
pub use view::{PlanSummary, RowClass, ShowFilters, SideTotals, ViewRow, is_visible, row_class, summarize, view_rows};

#[cfg(test)]
mod caps_test;
#[cfg(test)]
mod compare_test;
#[cfg(test)]
mod content_test;
#[cfg(test)]
mod control_test;
#[cfg(test)]
mod execute_fake;
#[cfg(test)]
mod execute_proptests;
#[cfg(test)]
mod execute_test;
#[cfg(test)]
mod filter_test;
#[cfg(test)]
mod guards_test;
#[cfg(test)]
mod keep_test;
#[cfg(test)]
mod key_test;
#[cfg(test)]
mod overrides_test;
#[cfg(test)]
mod plan_proptests;
#[cfg(test)]
mod plan_test;
#[cfg(test)]
mod profile_test;
#[cfg(test)]
mod scan_test;
#[cfg(test)]
mod steps_test;
#[cfg(test)]
mod test_tree;
#[cfg(test)]
mod view_test;
