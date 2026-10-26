//! The creation snapshot for sources the agent named one by one.
//!
//! A selector's ops carry the index row they matched. A hand-listed path has nothing behind it,
//! so without this every row of the agent's usual kind of group read "not in the index", even
//! on a freshly indexed drive. So each named source gets ONE bounded stat through its group's
//! source volume, before anything is written: a path that answers freezes its size, date, and
//! inode, and one that doesn't (missing, refused, too slow) stays `NULL`. Never a zero.
//!
//! The whole call shares ONE deadline, so a sweep onto a dead mount costs the agent one budget,
//! not one per file; the stats themselves are capped in `suggested_ops::facts`.

use std::sync::Arc;

use cmdr_fs::deadline::{Deadline, SESSION_IO_TIMEOUT};

use super::input::{PlannedOps, PlannedSources, PlannedSweep};
use crate::agent::suggested_ops::facts::stat_sources;
use crate::file_system::volume::Volume;

/// Fill the creation snapshot of every hand-listed source in `planned`, asking `volume_for`
/// for each group's source volume. A volume nobody registered leaves its group's snapshots
/// empty, which the review words as "no record".
pub(super) async fn freeze_named_sources(
    planned: &mut PlannedSweep,
    volume_for: impl Fn(&str) -> Option<Arc<dyn Volume>>,
) {
    let deadline = Deadline::new(SESSION_IO_TIMEOUT);
    for group in &mut planned.groups {
        match &mut group.ops {
            PlannedOps::Sources {
                sources: PlannedSources::Paths { ops, naming },
                ..
            } => {
                let Some(volume) = volume_for(&naming.source_volume_id) else {
                    continue;
                };
                let paths = ops.iter().map(|op| op.source_path.clone()).collect();
                for (op, stat) in ops.iter_mut().zip(stat_sources(volume, paths, &deadline).await) {
                    op.snapshot = stat.snapshot();
                }
            }
            PlannedOps::Rename { renames, naming, .. } => {
                let Some(volume) = volume_for(&naming.source_volume_id) else {
                    continue;
                };
                let paths = renames.iter().map(|op| op.source_path.clone()).collect();
                for (op, stat) in renames.iter_mut().zip(stat_sources(volume, paths, &deadline).await) {
                    op.snapshot = stat.snapshot();
                }
            }
            // A selector's ops carry the index row they matched; resolution fills them.
            PlannedOps::Sources {
                sources: PlannedSources::Selector(_),
                ..
            } => {}
        }
    }
}
