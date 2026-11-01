//! After the run: dated folders this run claimed but kept nothing in (an
//! earlier attempt's, or one the Trash policy emptied) are removed again,
//! children first, one `RemoveEmptyFolders` each, only after an observe shows
//! the folder empty. One that isn't, or that the engine refuses, stays and is
//! reported in `RunReport::leftover_folders`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{Runner, read};
use crate::{EngineBatch, EntryKind, ItemOutcome, Place, Side, SubtreeFacts};

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
            for dir in ordered {
                if !self.remove_if_empty(root.place, side, &dir).await {
                    self.leftover_folders.push((root.place, side, dir));
                }
            }
        }
    }

    /// Whether `dir` is gone: observed as an empty real folder, then removed
    /// by a non-recursive `rmdir`. Already gone counts as gone.
    async fn remove_if_empty(&self, place: Place, side: Side, dir: &Path) -> bool {
        let Some(now) = self.engine.observe(side, place, dir, true).await else {
            return true;
        };
        // The scan's counts skip OS junk, so this is the second line of
        // defence; the engine's `rmdir` refusing a non-empty folder is the first.
        let empty = now.facts.kind == EntryKind::Folder && now.subtree == Some(SubtreeFacts::default());
        if !empty {
            return false;
        }
        let batch = EngineBatch::RemoveEmptyFolders {
            place,
            side,
            dirs: vec![dir.to_path_buf()],
        };
        let outcome = self.engine.run(batch).await;
        // Only `Done` is a removal: `Skipped` (already gone, says the engine,
        // after the observe above saw it) is listed as left rather than guessed.
        read(&outcome, dir) == Some(ItemOutcome::Done)
    }
}
