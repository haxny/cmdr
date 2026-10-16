//! What the drive index knows, as the engine asks it: how much a selection
//! holds, how big a folder is, and which leaves sit under a subtree. The engine
//! never opens the index itself; Cmdr's answer reads it through
//! `operations_host::index`.
//!
//! Every answer is a best effort the engine can do without: `None`, or a
//! downgraded [`BufferedLeaves`], whenever the index doesn't cover the path.

use std::path::{Path, PathBuf};

use cmdr_operation_log::types::{EntryType, SearchCoverage, SearchCoverageReason};

/// The drive index's questions.
///
/// ❗ Each answer is a read of an in-process index, called per operation or per
/// top-level item, ❌ never per file a walk visits.
pub(crate) trait OperationIndex: Send + Sync {
    /// How many files and bytes `sources` hold in total, for a progress bar
    /// before the scan has counted them. `None` when any source isn't covered.
    fn expected_totals(&self, sources: &[PathBuf]) -> Option<ExpectedTotals>;

    /// A folder's recursive size, `None` when the index doesn't cover it. The
    /// engine never walks a tree to compute one.
    fn recursive_size(&self, path: &Path) -> Option<u64>;

    /// The leaves under `abs_source` on the index volume `volume_id`, at most
    /// `cap` of them, or a downgraded verdict with the honest reason. Read
    /// BEFORE the mutation that moves the subtree (`journal_search.rs`).
    fn search_leaves(&self, volume_id: &str, abs_source: &Path, cap: usize) -> BufferedLeaves;
}

/// What a selection holds, by the index's count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpectedTotals {
    /// How many files the sources contain, in total.
    pub files: u64,
    /// How many logical bytes they contain, in total.
    pub bytes: u64,
}

/// One enumerated descendant leaf, path RELATIVE to the subtree root. The caller
/// rebases it onto the source and dest roots when persisting.
#[derive(Debug, Clone)]
pub(crate) struct Leaf {
    pub rel: PathBuf,
    pub entry_type: EntryType,
    pub size: Option<i64>,
    pub mtime: Option<i64>,
}

/// The buffered result of enumerating a subtree BEFORE its mutation: the leaves to
/// persist on success, plus the coverage verdict (worst-case reason when
/// `top_level_only`). `Full` with an empty `leaves` means a leaf-less subtree that
/// IS fully covered (rare); a downgrade always carries an empty `leaves`.
#[derive(Debug)]
pub(crate) struct BufferedLeaves {
    pub coverage: SearchCoverage,
    pub reason: Option<SearchCoverageReason>,
    pub leaves: Vec<Leaf>,
}

impl BufferedLeaves {
    /// A `top_level_only` downgrade with the given honest reason and no leaves.
    pub(crate) fn downgraded(reason: SearchCoverageReason) -> Self {
        Self {
            coverage: SearchCoverage::TopLevelOnly,
            reason: Some(reason),
            leaves: Vec::new(),
        }
    }

    /// Full coverage with the enumerated leaves.
    pub(crate) fn full(leaves: Vec<Leaf>) -> Self {
        Self {
            coverage: SearchCoverage::Full,
            reason: None,
            leaves,
        }
    }
}

/// The answer for a host with no index: nothing is covered, and a subtree's
/// leaves downgrade the way an unindexed volume's do.
pub(super) struct NoOperationIndex;

impl OperationIndex for NoOperationIndex {
    fn expected_totals(&self, _sources: &[PathBuf]) -> Option<ExpectedTotals> {
        None
    }

    fn recursive_size(&self, _path: &Path) -> Option<u64> {
        None
    }

    fn search_leaves(&self, _volume_id: &str, _abs_source: &Path, _cap: usize) -> BufferedLeaves {
        BufferedLeaves::downgraded(SearchCoverageReason::VolumeNotLive)
    }
}
