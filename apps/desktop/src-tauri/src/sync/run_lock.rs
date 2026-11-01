//! One run per folder at a time, across sessions: a run holds both its roots,
//! and a new run is refused when either of its roots is, holds, or lies inside
//! a root a run holds. Roots in this Mac's namespace compare by their real
//! path whatever volume id named them; others by volume and path.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::session::SideSetup;
use super::wire::SyncError;

/// The namespace every local-disk and mounted-share root shares.
const LOCAL_NAMESPACE: &str = "local-fs";

/// One run going on: its lock token and its two roots.
type Held = (u64, [RootKey; 2]);

/// The runs going on.
static RUNNING: LazyLock<Mutex<Vec<Held>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static TOKENS: AtomicU64 = AtomicU64::new(0);

/// A root, normalised: its namespace and its path components (no empty or `.`
/// parts, so `/a/` is `/a`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootKey {
    namespace: String,
    parts: Vec<String>,
}

impl RootKey {
    fn new(namespace: &str, path: &Path) -> Self {
        let parts = path
            .to_string_lossy()
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .map(str::to_owned)
            .collect();
        Self {
            namespace: namespace.to_owned(),
            parts,
        }
    }

    /// The key of a side: its real path when it has one, else volume and path.
    pub(crate) fn of(side: &SideSetup) -> Self {
        match &side.real_root {
            Some(real) => Self::new(LOCAL_NAMESPACE, real),
            None => Self::new(&side.root_ref.volume_id, &side.root),
        }
    }

    /// The same folder, or one inside the other.
    fn overlaps(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && (self.parts.starts_with(&other.parts) || other.parts.starts_with(&self.parts))
    }
}

/// Holds its roots until dropped (a panicking run releases them too).
pub(crate) struct RunLock(u64);

impl RunLock {
    /// The lock for these roots, or `RunInProgress` when a run holds one of
    /// them, a folder inside one, or a folder around one.
    pub(crate) fn take(roots: [RootKey; 2]) -> Result<Self, SyncError> {
        let mut running = RUNNING.lock_ignore_poison();
        let busy = running
            .iter()
            .any(|(_, held)| held.iter().any(|held| roots.iter().any(|root| root.overlaps(held))));
        if busy {
            return Err(SyncError::RunInProgress);
        }
        let token = TOKENS.fetch_add(1, Ordering::Relaxed);
        running.push((token, roots));
        Ok(Self(token))
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        RUNNING.lock_ignore_poison().retain(|(token, _)| *token != self.0);
    }
}

#[cfg(test)]
pub(crate) fn key(namespace: &str, path: &str) -> RootKey {
    RootKey::new(namespace, Path::new(path))
}
