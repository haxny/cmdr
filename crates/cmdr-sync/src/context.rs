//! What a run needs beyond the plan: roots, volumes, the keep policy, and the
//! versions folders that policy resolves to.

use std::path::{Path, PathBuf};

use crate::{KeepPolicy, KeepRefusal, Place, RunStamp, Side, SyncCaps, VersionsRoot, staging_root, versions_root};

/// One side of a run, as `RunContext::new` takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSide {
    /// The folder.
    pub root: PathBuf,
    /// What its volume can do now.
    pub caps: SyncCaps,
    /// Whether `root` is a path in this Mac's own file namespace (the local
    /// disk or a mounted share), so a custom folder on the local disk can lie
    /// inside it. A remote volume's path (`/R` on an SFTP server) never can.
    pub local_namespace: bool,
}

impl RunSide {
    /// A side whose root is in the local file namespace.
    pub fn local(root: PathBuf, caps: SyncCaps) -> Self {
        Self {
            root,
            caps,
            local_namespace: true,
        }
    }
}

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
    /// Which roots are in the local file namespace (left, right): only those
    /// can hold a custom folder on the local disk.
    pub local_roots: [bool; 2],
}

/// The policy's folder and the staging folder for one side.
fn resolve(
    keep: &KeepPolicy,
    root: &Path,
    profile: Option<&str>,
    stamp: &RunStamp,
    attempt: u32,
    local_roots: &[&Path],
) -> Result<(VersionsRoot, VersionsRoot), KeepRefusal> {
    let versions = versions_root(keep, root, profile, stamp, attempt)?;
    let inside = versions.place == Place::LocalDisk && local_roots.iter().any(|sync| versions.dir.starts_with(sync));
    if inside {
        return Err(KeepRefusal::InsideSyncFolder);
    }
    Ok((versions, staging_root(keep, root, stamp, attempt)?))
}

impl RunContext {
    /// A context with first-attempt versions folders. A custom folder inside
    /// either root is refused (`InsideSyncFolder`): its contents would be
    /// compared and synchronized like the person's files. Only a root in the
    /// local file namespace is checked: a remote path can't hold a local one.
    pub fn new(
        left: RunSide,
        right: RunSide,
        keep: KeepPolicy,
        stamp: RunStamp,
        profile_name: Option<String>,
    ) -> Result<Self, KeepRefusal> {
        let local_roots = [left.local_namespace, right.local_namespace];
        let roots = Self::local_paths([&left.root, &right.root], local_roots);
        let profile = profile_name.as_deref();
        let (versions_left, staging_left) = resolve(&keep, &left.root, profile, &stamp, 0, &roots)?;
        let (versions_right, staging_right) = resolve(&keep, &right.root, profile, &stamp, 0, &roots)?;
        Ok(Self {
            left_root: left.root,
            right_root: right.root,
            left_caps: left.caps,
            right_caps: right.caps,
            keep,
            stamp,
            profile_name,
            versions_left,
            versions_right,
            staging_left,
            staging_right,
            local_roots,
        })
    }

    /// The roots a custom folder on the local disk must stay out of.
    fn local_paths(roots: [&Path; 2], local: [bool; 2]) -> Vec<&Path> {
        roots
            .into_iter()
            .zip(local)
            .filter_map(|(root, local)| local.then_some(root))
            .collect()
    }

    /// The same context with `side`'s folders named for `attempt` (a dated
    /// folder that already exists is never reused).
    pub fn with_attempt(&self, side: Side, attempt: u32) -> Result<Self, KeepRefusal> {
        let roots = Self::local_paths([&self.left_root, &self.right_root], self.local_roots);
        let profile = self.profile_name.as_deref();
        let (versions, staging) = resolve(&self.keep, self.root(side), profile, &self.stamp, attempt, &roots)?;
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
