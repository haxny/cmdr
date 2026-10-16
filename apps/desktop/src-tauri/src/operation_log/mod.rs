//! The operation log: a durable, cross-volume journal of every file mutation,
//! the foundation for search, rollback, and a future undo.
//!
//! This module is the DURABLE STORE: the schema, the forward-migration
//! ladder, the single writer thread, and dir interning. The capture layer, the
//! rollback engine, search/retention, the MCP tools, and the UI build on it.
//!
//! Unlike every other on-disk store in the app (the drive index and
//! `importance.db` are disposable per-volume caches that delete-and-recreate on
//! a schema bump), this DB lives for years, so it introduces the codebase's
//! first forward-migration ladder (`store`'s migration ladder) and retention
//! discipline. Design rationale, the migration-ladder template, and the capture/
//! rollback contracts: `CLAUDE.md` + `DETAILS.md`.

pub mod capture;
pub mod query;
pub mod retention;
pub mod rollback;
pub mod store;
pub mod types;
pub mod writer;

mod handle;
mod recording;

pub use handle::{OpJournal, OpenError, OperationLog};

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch — the journal's opaque clock (the store owns no
/// clock, so callers supply the time). Shared by the write pipeline's capture glue
/// and the rollback engine.
pub(crate) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Mint an operation id.
///
/// **UUIDv7, never v4, and that is load-bearing.** This clock is whole seconds,
/// so two operations started in the same second tie on `started_at` and every
/// ordered read falls back to `op_id` — the history feed, the retention
/// watermark, and the startup reconcile's "which inverse op is the latest one".
/// A v7's leading 48 bits are a big-endian millisecond timestamp, so its hex
/// rendering sorts chronologically and that fallback becomes the right answer
/// instead of a coin flip. A v4 id makes the same-second order random: history
/// showing a reversal above the operation it reversed, and a reconcile that can
/// read the WRONG inverse op's outcomes.
///
/// Every managed operation, and every inverse operation, takes its id from here.
pub fn new_operation_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
