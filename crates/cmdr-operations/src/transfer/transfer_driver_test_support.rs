//! Shared fixtures for the `transfer_driver.rs` test suites
//! (`transfer_driver_pre_skip_tests.rs`, `transfer_driver_sync_tests.rs`,
//! `transfer_driver_async_tests.rs`, `transfer_driver_concurrent_tests.rs`).
//!
//! Items are `pub(super)` so the sibling test modules (all children of the
//! `transfer_driver` module) can reach them through `super::test_support::…`.

use super::super::super::state::WriteOperationState;
use super::super::super::types::{ConflictResolution, TransferLeaves, WriteOperationPhase, WriteOperationType};
use super::DriverConfig;
use crate::testing::TestOperations;
use cmdr_fs::ignore_poison::IgnorePoison;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) fn make_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    // Zero progress interval so throttled emits ALWAYS fire — tests that
    // count emits would otherwise be flaky.
    Arc::new(engine.new_operation_state(Duration::from_millis(0)))
}

pub(super) fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

pub(super) fn copy_config() -> DriverConfig {
    DriverConfig {
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        conflict_resolution: ConflictResolution::Stop,
        pre_known_conflicts: Vec::new(),
        // Driver unit tests use bare closures that emit no progress of their
        // own, so they exercise the driver's per-source milestone path.
        emit_per_source_milestone: true,
        skip_ledger: None,
        report_source_outcomes: false,
        leaves: TransferLeaves::default(),
    }
}

/// Tiny in-memory "call log" the closures dump into so tests can assert
/// invocation order and counts.
#[derive(Default)]
pub(super) struct CallLog {
    invoked_for: Mutex<Vec<PathBuf>>,
    invoked_dests: Mutex<Vec<Option<PathBuf>>>,
}

impl CallLog {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub(super) fn record(&self, src: &Path, dest: Option<&Path>) {
        self.invoked_for.lock_ignore_poison().push(src.to_path_buf());
        self.invoked_dests
            .lock_ignore_poison()
            .push(dest.map(|p| p.to_path_buf()));
    }
    pub(super) fn sources(&self) -> Vec<PathBuf> {
        self.invoked_for.lock_ignore_poison().clone()
    }
    pub(super) fn dests(&self) -> Vec<Option<PathBuf>> {
        self.invoked_dests.lock_ignore_poison().clone()
    }
}
