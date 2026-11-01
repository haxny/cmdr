//! What a run needs beyond the plan: roots, volumes, the keep policy, and the
//! versions folders that policy resolves to.

use std::path::{Path, PathBuf};

use crate::{KeepPolicy, KeepRefusal, Place, RunStamp, Side, SyncCaps, VersionsRoot, staging_root, versions_root};

/// Everything a run needs beyond the plan. Build it with `RunContext::new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunContext {
    /// The left folder.
    pub left_root: PathBuf,
    /// The right folder.
    pub right_root: PathBuf,
    /// What the left volume can do now.
    pub left_caps: SyncCaps,
    /// What the right volume can do now.
    pub right_caps: SyncCaps,
    /// How replaced and removed items are kept.
    pub keep: KeepPolicy,
    /// When the run started.
    pub stamp: RunStamp,
    /// The profile's name, for `{profile}`.
    pub profile_name: Option<String>,
    /// The policy's versions folder for items kept from the left.
    pub versions_left: VersionsRoot,
    /// The policy's versions folder for items kept from the right.
    pub versions_right: VersionsRoot,
    /// Where files replaced on the left wait for their copy (same volume).
    pub staging_left: VersionsRoot,
    /// Where files replaced on the right wait for their copy (same volume).
    pub staging_right: VersionsRoot,
}

/// The policy's folder and the staging folder for one side.
fn resolve(
    keep: &KeepPolicy,
    root: &Path,
    profile: Option<&str>,
    stamp: &RunStamp,
    attempt: u32,
    sync_roots: [&Path; 2],
) -> Result<(VersionsRoot, VersionsRoot), KeepRefusal> {
    let versions = versions_root(keep, root, profile, stamp, attempt)?;
    let inside = versions.place == Place::LocalDisk && sync_roots.iter().any(|sync| versions.dir.starts_with(sync));
    if inside {
        return Err(KeepRefusal::InsideSyncFolder);
    }
    Ok((versions, staging_root(keep, root, stamp, attempt)?))
}

impl RunContext {
    /// A context with first-attempt versions folders. A custom folder inside
    /// either root is refused (`InsideSyncFolder`): its contents would be
    /// compared and synchronized like the person's files.
    pub fn new(
        left_root: PathBuf,
        right_root: PathBuf,
        left_caps: SyncCaps,
        right_caps: SyncCaps,
        keep: KeepPolicy,
        stamp: RunStamp,
        profile_name: Option<String>,
    ) -> Result<Self, KeepRefusal> {
        let roots = [left_root.as_path(), right_root.as_path()];
        let profile = profile_name.as_deref();
        let (versions_left, staging_left) = resolve(&keep, &left_root, profile, &stamp, 0, roots)?;
        let (versions_right, staging_right) = resolve(&keep, &right_root, profile, &stamp, 0, roots)?;
        Ok(Self {
            left_root,
            right_root,
            left_caps,
            right_caps,
            keep,
            stamp,
            profile_name,
            versions_left,
            versions_right,
            staging_left,
            staging_right,
        })
    }

    /// The same context with `side`'s folders named for `attempt` (a dated
    /// folder that already exists is never reused).
    pub fn with_attempt(&self, side: Side, attempt: u32) -> Result<Self, KeepRefusal> {
        let roots = [self.left_root.as_path(), self.right_root.as_path()];
        let profile = self.profile_name.as_deref();
        let (versions, staging) = resolve(&self.keep, self.root(side), profile, &self.stamp, attempt, roots)?;
        let mut next = self.clone();
        match side {
            Side::Left => (next.versions_left, next.staging_left) = (versions, staging),
            Side::Right => (next.versions_right, next.staging_right) = (versions, staging),
        }
        Ok(next)
    }

    /// The root of `side`.
    pub fn root(&self, side: Side) -> &Path {
        match side {
            Side::Left => &self.left_root,
            Side::Right => &self.right_root,
        }
    }

    /// What `side`'s volume can do.
    pub fn caps(&self, side: Side) -> &SyncCaps {
        match side {
            Side::Left => &self.left_caps,
            Side::Right => &self.right_caps,
        }
    }

    /// The policy's versions folder for items kept from `side`.
    pub fn versions(&self, side: Side) -> &VersionsRoot {
        match side {
            Side::Left => &self.versions_left,
            Side::Right => &self.versions_right,
        }
    }

    /// Where files replaced on `side` wait for their copy.
    pub fn staging(&self, side: Side) -> &VersionsRoot {
        match side {
            Side::Left => &self.staging_left,
            Side::Right => &self.staging_right,
        }
    }
}
