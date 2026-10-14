//! Streamed commit log under `.git/commits/`.
//!
//! Each commit becomes a virtual directory named with its short SHA. The
//! display name is `<short-sha> <subject>` so date / name sorts read well.
//! `addedAt` and `createdAt` carry the author date so date-sort works out
//! of the box.
//!
//! ## HEAD-reachable, capped silently, counts for the newest only
//!
//! `list_commits` walks the rev graph from HEAD by commit time (newest
//! first) and stops silently at `MAX_COMMITS` (100,000). Only the newest
//! `EAGER_FILES_CHANGED` (5,000) rows get their files-changed count during
//! the walk: that diff is ~90% of a row's cost, and it's what makes Size
//! sort work there. Rows past it arrive with no count, and the host fills
//! in the ones a pane shows through `commit_rows_with_counts`. Numbers:
//! `DETAILS.md` § "Modified + Size columns for virtual entries".
//!
//! ## Direct path entry to any commit
//!
//! `.git/commits/<sha>/...` resolves to that commit's tree even when the
//! SHA isn't in the listing. This is essential for typed-in SHAs and for
//! shallow-clone unreachable commits. Implemented in `path::classify` by
//! treating any 7+ hex segment as a commit ref. The actual commit object
//! is resolved lazily in `resolve_commit_id` so unknown commits surface a
//! friendly error instead of swallowing.
//!
//! ## Progress and cancellation: one `WalkControl` per listing
//!
//! `list_directory` answers once, with every row, so the walk reports how
//! far it got through a `WalkControl` the volume samples on a timer and
//! hands to the pane as progress. The same control carries "abandoned":
//! a `spawn_blocking` task can't be aborted, so without it a pane that left
//! a 100,000-commit listing would keep the walk burning CPU for nobody. Per
//! listing, ❌ never a static: two listings must not cancel each other.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use gix::ObjectId;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

use cmdr_fs::entry::FileEntry;

use crate::column_meta::files_changed_count;
use crate::repo::RepoHandle;
use cmdr_fs::git_meta::{GitCountKind, GitEntryMeta};
use cmdr_fs::volume::friendly_error::git::{FriendlyGitError, FriendlyGitErrorKind};

/// Hard cap on entries returned in a single `commits/` listing.
///
/// Without the eager count a row costs ~11 µs and ~576 bytes (Cmdr's 10,646
/// commits in 116 ms; release build, M-series, 2026-10-14), so the cap is
/// ~1 s plus `EAGER_FILES_CHANGED`'s share, and ~58 MB of listing cache. The
/// progress ticks keep that wait honest.
pub const MAX_COMMITS: usize = 100_000;

/// How many of the newest rows get their files-changed count during the walk,
/// which is what Size sort orders them by. ~230 µs each on Cmdr's own repo
/// (CPU time, so load can't inflate it; release build, M-series,
/// 2026-10-14), so ~1.2 s for the whole budget.
pub const EAGER_FILES_CHANGED: usize = 5_000;

/// How many hex chars we display per short SHA. Matches `git log --oneline`.
pub const SHORT_SHA_LEN: usize = 7;

/// A commit walk's live state, shared between the blocking walk and the async
/// side that started it. See the module docs.
// DEFAULT-OK: zero rows listed and not abandoned is exactly a walk that hasn't started.
#[derive(Debug, Default)]
pub struct WalkControl {
    listed: AtomicUsize,
    abandoned: AtomicBool,
}

impl WalkControl {
    /// How many commits the walk has listed so far.
    pub fn listed(&self) -> usize {
        self.listed.load(Ordering::Relaxed)
    }

    /// Tells the walk nobody wants its rows any more; it stops at the next commit.
    pub fn abandon(&self) {
        self.abandoned.store(true, Ordering::Relaxed);
    }

    fn is_abandoned(&self) -> bool {
        self.abandoned.load(Ordering::Relaxed)
    }
}

/// Resolves a SHA-ish path segment to a commit `ObjectId`.
///
/// Accepts a full 40-char hex or any prefix ≥ `SHORT_SHA_LEN` chars (gix
/// resolves it via the loose-object index). Returns a friendly error if
/// the prefix is ambiguous or the object doesn't exist or isn't a commit.
pub fn resolve_commit_id(handle: &RepoHandle, prefix: &str) -> Result<ObjectId, FriendlyGitError> {
    let repo = handle.to_thread_local();
    let rev = repo
        .rev_parse_single(prefix)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let id = rev.detach();
    let obj = repo
        .find_object(id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    if obj.kind != gix::object::Kind::Commit {
        return Err(FriendlyGitError::new(
            FriendlyGitErrorKind::CorruptRepo,
            prefix.to_string(),
        ));
    }
    Ok(id)
}

/// Lists HEAD-reachable commits as virtual directory entries.
///
/// Up to `MAX_COMMITS` entries; on cap the walk stops silently. The newest
/// `EAGER_FILES_CHANGED` carry their files-changed count. Reports progress
/// and honors abandonment through `control`.
pub fn list_commits(
    handle: &RepoHandle,
    repo_root: &Path,
    control: &WalkControl,
) -> Result<Vec<FileEntry>, FriendlyGitError> {
    walk_commits(handle, repo_root, control, EAGER_FILES_CHANGED)
}

/// [`list_commits`] with the eager-count budget as a parameter, so a test can
/// put the boundary inside a five-commit fixture.
pub(crate) fn walk_commits(
    handle: &RepoHandle,
    repo_root: &Path,
    control: &WalkControl,
    eager_counts: usize,
) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join("commits");
    let repo = handle.to_thread_local();

    let head_id = match repo.head_id() {
        Ok(id) => id.detach(),
        Err(_) => {
            // Unborn HEAD: empty `commits/` listing is correct.
            return Ok(Vec::new());
        }
    };

    let walk = repo
        .rev_walk([head_id])
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    let mut out = Vec::new();
    for (count, info) in walk.enumerate() {
        if count >= MAX_COMMITS || control.is_abandoned() {
            break;
        }
        let info =
            info.map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
        out.push(commit_row(&repo, &parent, info.id, count < eager_counts)?);
        control.listed.store(out.len(), Ordering::Relaxed);
    }
    // An abandoned walk's rows go nowhere; returning a part of them would
    // only look like a short history.
    if control.is_abandoned() {
        out.clear();
    }

    Ok(out)
}

/// The `.git/commits/<sha>` rows at `paths` rebuilt WITH their files-changed
/// count, for the rows past `EAGER_FILES_CHANGED` that a pane shows. A path
/// that isn't a commit row of this repo, or whose commit doesn't resolve, is
/// left out.
pub fn commit_rows_with_counts(handle: &RepoHandle, repo_root: &Path, paths: &[PathBuf]) -> Vec<FileEntry> {
    let parent = repo_root.join(".git").join("commits");
    let repo = handle.to_thread_local();
    paths
        .iter()
        .filter_map(|path| {
            if path.parent() != Some(parent.as_path()) {
                return None;
            }
            let short = path.file_name()?.to_str()?;
            let id = resolve_commit_id(handle, short).ok()?;
            commit_row(&repo, &parent, id, true).ok()
        })
        .collect()
}

/// One commit's row: `<short-sha> <subject>`, its dates, and (when
/// `with_count`) its files-changed count as Size.
fn commit_row(
    repo: &gix::Repository,
    parent: &Path,
    id: ObjectId,
    with_count: bool,
) -> Result<FileEntry, FriendlyGitError> {
    // Author date drives display time. We pull the commit object once for the
    // subject line and the author signature; gix caches loose objects so the
    // second hit is cheap.
    let (subject, author_secs, committer_secs) = decode_commit_meta(repo, id)?;

    let short = short_sha(&id);
    let entry_path = parent.join(&short);
    let mut fe = FileEntry::new(short.clone(), entry_path.to_string_lossy().into_owned(), true, false);
    fe.icon_id = "git:commit".into();
    fe.permissions = 0o755;
    // Display: `<short-sha> <subject>` – same shape as `git log --oneline` so
    // users feel at home.
    fe.name = format!("{} {}", short, subject);
    fe.modified_at = Some(committer_secs as u64);
    fe.created_at = Some(author_secs as u64);
    fe.added_at = Some(author_secs as u64);
    // Files-changed vs. first parent. For an initial commit (no parent),
    // `files_changed_count` returns the total tree entry count, still a useful
    // "size of this snapshot" cue.
    fe.git_meta = if with_count {
        files_changed_count(repo, id).map(|n| {
            fe.size = Some(n);
            GitEntryMeta::Count {
                counted: GitCountKind::FilesChanged,
                n,
            }
        })
    } else {
        Some(GitEntryMeta::CountPending {
            counted: GitCountKind::FilesChanged,
        })
    };
    Ok(fe)
}

fn decode_commit_meta(repo: &gix::Repository, id: ObjectId) -> Result<(String, i64, i64), FriendlyGitError> {
    let commit = repo
        .find_commit(id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let message = commit
        .message_raw()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    // Subject = first line of the commit message; truncate to keep the
    // listing readable (long subjects break the file-list layout).
    let subject = message
        .to_string()
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(80)
        .collect::<String>();
    let author = commit
        .author()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let committer = commit
        .committer()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let author_time = author
        .time()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let committer_time = committer
        .time()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    Ok((subject, author_time.seconds, committer_time.seconds))
}

fn short_sha(id: &ObjectId) -> String {
    id.to_string().chars().take(SHORT_SHA_LEN).collect()
}
