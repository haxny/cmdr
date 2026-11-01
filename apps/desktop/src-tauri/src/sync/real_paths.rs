//! Real paths in this Mac's namespace (symlinks and aliases resolved), always
//! on a blocking thread with a time limit (a hung mount blocks `realpath`), and
//! never under a session lock. Used at compare (the roots), at confirm (a custom
//! versions folder), and again at run (both: a root swapped since the compare is
//! a stale compare).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::{Volume, root_anchored};
use cmdr_sync::{KeepRefusal, Place, RunContext, Side};

use super::session::{Compared, SideSetup};
use super::wire::SyncError;

/// How long resolving one path may take.
const REAL_PATH_WAIT: Duration = Duration::from_secs(5);

/// Why a path couldn't be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unresolved {
    /// It didn't answer in time, or answered with an error other than "not there".
    Unreadable,
}

/// Runs `resolve` on a blocking thread, giving up after `REAL_PATH_WAIT`.
async fn blocking<T: Send + 'static>(
    resolve: impl FnOnce() -> Result<T, Unresolved> + Send + 'static,
) -> Result<T, Unresolved> {
    match tokio::time::timeout(REAL_PATH_WAIT, tokio::task::spawn_blocking(resolve)).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) | Err(_) => Err(Unresolved::Unreadable),
    }
}

/// Where `root` on `volume` really is, for a volume with a local path;
/// `Ok(None)` for a remote one, whose paths aren't this Mac's.
pub(crate) async fn real_root(volume: &Arc<dyn Volume>, root: &Path) -> Result<Option<PathBuf>, Unresolved> {
    let Some(local) = volume.local_path() else {
        return Ok(None);
    };
    let os_path = root_anchored(&local, root);
    blocking(move || std::fs::canonicalize(os_path).map_err(|_| Unresolved::Unreadable))
        .await
        .map(Some)
}

/// `path` with its deepest existing ancestor resolved. Only "not there" climbs
/// to the parent; any other error refuses, since a folder that won't answer
/// can't be shown to lie outside the roots.
fn real_path_now(path: &Path) -> Result<PathBuf, Unresolved> {
    let mut rest = Vec::new();
    for ancestor in path.ancestors() {
        match std::fs::canonicalize(ancestor) {
            Ok(real) => return Ok(rest.iter().rev().fold(real, |path: PathBuf, part| path.join(part))),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err(Unresolved::Unreadable),
        }
        if let Some(name) = ancestor.file_name() {
            rest.push(name.to_owned());
        }
    }
    Err(Unresolved::Unreadable)
}

/// What to resolve, read under the session lock and checked after it.
pub(crate) struct PathCheck {
    /// Each local root and the real path the compare saw.
    roots: Vec<(Side, Arc<dyn Volume>, PathBuf, PathBuf)>,
    /// Custom versions folders on the local disk.
    custom: Vec<PathBuf>,
    /// The roots' real paths, which no custom folder may lie in.
    real_roots: Vec<PathBuf>,
}

impl PathCheck {
    /// The roots (when `roots`) and every custom versions folder of `ctx`.
    pub(crate) fn of(compared: &Compared, ctx: &RunContext, roots: bool) -> Self {
        let side = |side: Side, setup: &SideSetup| {
            let real = setup.real_root.clone()?;
            Some((side, Arc::clone(&setup.volume), setup.root.clone(), real))
        };
        let local_roots = if roots {
            [side(Side::Left, &compared.left), side(Side::Right, &compared.right)]
                .into_iter()
                .flatten()
                .collect()
        } else {
            Vec::new()
        };
        let custom = [Side::Left, Side::Right]
            .into_iter()
            .map(|side| ctx.versions(side))
            .filter(|versions| versions.place == Place::LocalDisk)
            .map(|versions| versions.dir.clone())
            .collect();
        let real_roots = [&compared.left.real_root, &compared.right.real_root]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        Self {
            roots: local_roots,
            custom,
            real_roots,
        }
    }

    /// Every root still resolves to the real path the compare saw (else the
    /// compare is stale, or the root unreachable), and no custom folder lies,
    /// really, inside a root.
    pub(crate) async fn verify(self) -> Result<(), SyncError> {
        for (side, volume, root, seen) in self.roots {
            match real_root(&volume, &root).await {
                Ok(Some(now)) if now == seen => {}
                Ok(_) => return Err(SyncError::StaleCompare),
                Err(Unresolved::Unreadable) => return Err(SyncError::RootUnreachable { side }),
            }
        }
        for dir in self.custom {
            let real = blocking(move || real_path_now(&dir))
                .await
                .map_err(|Unresolved::Unreadable| SyncError::KeepFolderUnreadable)?;
            if self.real_roots.iter().any(|root| real.starts_with(root)) {
                let refusal = KeepRefusal::InsideSyncFolder;
                return Err(SyncError::Keep { refusal });
            }
        }
        Ok(())
    }
}
