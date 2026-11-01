//! After the run: dated folders this run claimed but kept nothing in (an
//! earlier attempt's, or one the Trash policy emptied) are removed again,
//! children first, with `RemoveEmptyFolders` (only ever empty folders).

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::Runner;
use crate::EngineBatch;

impl Runner<'_> {
    pub(super) async fn tidy(&mut self) {
        let claimed = self.claimed.clone();
        for (side, root) in claimed {
            if self.versions_used.contains(&root) {
                continue;
            }
            let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
            for (place, s, dir) in &self.keep_dirs {
                if *place != root.place || *s != side || !dir.starts_with(&root.dir) {
                    continue;
                }
                dirs.extend(
                    dir.ancestors()
                        .take_while(|ancestor| ancestor.starts_with(&root.dir))
                        .map(PathBuf::from),
                );
            }
            dirs.insert(root.dir.clone());
            // Deepest first, so each folder is empty by the time it's asked.
            let mut ordered: Vec<PathBuf> = dirs.into_iter().collect();
            ordered.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
            let batch = EngineBatch::RemoveEmptyFolders {
                place: root.place,
                side,
                dirs: ordered,
            };
            self.engine.run(batch).await;
        }
    }
}
