//! What a group's sources look like on disk, and how far a group can really be taken back.
//!
//! Two reads of the same thing at two moments: the CREATION SNAPSHOT a group freezes when it's
//! proposed, and the LIVE stat the review dialog shows beside it. Both go through the group's
//! SOURCE volume, so a file on a share is asked of the share, never of a guess about its mount.
//!
//! Every stat is bounded twice: by one shared [`Deadline`] per call (a dead mount costs the
//! caller one budget, not one per file), and by [`SOURCE_STATS`], which caps how many stats are
//! in flight across every caller. The dialog re-asks on scroll faster than a slow share answers,
//! and a stat that outlives its deadline keeps its permit until it really ends, so stragglers on
//! a dead mount can't pile up pool threads behind the user's back.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::deadline::{BlockingBudget, Deadline, io_budget, timeout_detached_within};
use cmdr_fs::log_error::LogError;
use futures_util::StreamExt;

use crate::agent::store::proposals::OpSnapshot;
use crate::agent::types::{ProposalVerb, Reversibility};
use crate::file_system::volume::{Volume, VolumeError};

/// How long one batch of stats may take on a local disk or a kernel mount. A volume with a live
/// session gets [`io_budget`]'s longer wait instead, since its transport tells slow from dead.
pub const SOURCE_STAT_TIMEOUT: Duration = Duration::from_secs(2);

/// How many stats one call keeps in flight. Small on purpose: a share answers a handful of
/// concurrent stats no slower than one, and a page is 50-ish rows.
const STATS_PER_CALL: usize = 8;

/// Every source stat this module runs, from every caller (the proposal's snapshot and the
/// dialog's live facts contend for the same volumes). Sized to what a NAS can usefully serve.
static SOURCE_STATS: BlockingBudget = BlockingBudget::new(8);

/// One source as its volume answers for it right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStat {
    /// It's there. `size` is `None` for a folder, whose own stat size means nothing to a user.
    Present {
        size: Option<u64>,
        /// Unix seconds.
        modified: Option<i64>,
        inode: Option<u64>,
    },
    /// The volume says there's nothing at that path.
    Missing,
    /// The volume didn't answer in time, or refused in a way that isn't "not there".
    Unknown,
}

impl SourceStat {
    /// The creation snapshot this stat freezes into, or `None` when there was nothing to
    /// freeze. A missing or unanswered stat stays `NULL`, ❌ never a zero.
    pub fn snapshot(self) -> Option<OpSnapshot> {
        match self {
            SourceStat::Present { size, modified, inode } => Some(OpSnapshot {
                size,
                mtime: modified,
                inode,
            }),
            SourceStat::Missing | SourceStat::Unknown => None,
        }
    }
}

/// The deadline a batch of stats on `volume` shares.
pub fn stat_deadline(volume: &dyn Volume) -> Deadline {
    Deadline::new(io_budget(volume.connection_state(), SOURCE_STAT_TIMEOUT))
}

/// Stat every path on `volume` within `deadline`, answering in the order asked.
pub async fn stat_sources(volume: Arc<dyn Volume>, paths: Vec<String>, deadline: &Deadline) -> Vec<SourceStat> {
    futures_util::stream::iter(paths)
        .map(|path| stat_one(volume.clone(), path, deadline))
        .buffered(STATS_PER_CALL)
        .collect()
        .await
}

/// Why a stat gave no answer. Only ever logged.
#[derive(Debug)]
enum StatGap {
    TimedOut,
    Panicked(String),
}

async fn stat_one(volume: Arc<dyn Volume>, path: String, deadline: &Deadline) -> SourceStat {
    // A stat that only got its permit after the caller stopped waiting is work for nobody, so it
    // doesn't start. The instant is fixed here because the detached task outlives `deadline`.
    let give_up_at = tokio::time::Instant::now() + deadline.remaining();
    let runtime = tokio::runtime::Handle::current();
    let answer = timeout_detached_within(deadline, || StatGap::TimedOut, StatGap::Panicked, async move {
        SOURCE_STATS
            .run(move || {
                if tokio::time::Instant::now() >= give_up_at {
                    return SourceStat::Unknown;
                }
                // On a blocking thread, holding the budget's permit until the stat really
                // ends: that's what keeps a dead mount's stragglers counted.
                runtime.block_on(async move { classify(volume.get_metadata(Path::new(&path)).await) })
            })
            .await
            .map_err(|e| StatGap::Panicked(e.to_string()))
    })
    .await;
    match answer {
        Ok(stat) => stat,
        Err(StatGap::TimedOut) => {
            log::debug!(
                target: "agent::suggested_ops",
                "a source stat didn't answer within {:?}",
                deadline.total()
            );
            SourceStat::Unknown
        }
        Err(StatGap::Panicked(detail)) => {
            log::warn!(target: "agent::suggested_ops", "a source stat panicked: {detail}");
            SourceStat::Unknown
        }
    }
}

fn classify(answer: Result<cmdr_fs::entry::FileEntry, VolumeError>) -> SourceStat {
    match answer {
        Ok(entry) => SourceStat::Present {
            size: if entry.is_directory { None } else { entry.size },
            modified: entry.modified_at.map(|at| at as i64),
            inode: entry.inode,
        },
        Err(VolumeError::NotFound(_)) => SourceStat::Missing,
        Err(e) => {
            log::debug!(target: "agent::suggested_ops", "a source stat was refused: {:?}", LogError(&e));
            SourceStat::Unknown
        }
    }
}

/// How far an approved group can really be taken back, given what its source volume can do.
///
/// `stored` is the intent's answer (`GroupIntent::reversibility`, the per-VERB part, frozen at
/// creation). The per-VOLUME part is only known at read time, because a volume's capabilities
/// change while a suggestion waits: trash is undoable only where there IS a trash. So a trash
/// group promises undo only when `source_has_trash` says yes; an unknown answer promises
/// nothing, because a promise the app can't keep is the worse error.
pub fn reversibility_on_source(
    verb: ProposalVerb,
    stored: Reversibility,
    source_has_trash: Option<bool>,
) -> Reversibility {
    match (verb, source_has_trash) {
        (ProposalVerb::Trash, Some(true)) => stored,
        (ProposalVerb::Trash, _) => Reversibility::Irreversible,
        _ => stored,
    }
}

/// The folder every path sits directly in, or `None` when they don't all share one.
///
/// Streams: it holds one path at a time, so a 60 000-op group costs a pass, not a load.
pub fn shared_parent<I, S>(paths: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut shared: Option<String> = None;
    for path in paths {
        let parent = Path::new(path.as_ref()).parent()?.to_str()?;
        match &shared {
            None => shared = Some(parent.to_string()),
            Some(seen) if seen == parent => {}
            Some(_) => return None,
        }
    }
    shared
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::volume::InMemoryVolume;

    /// Trash promises undo only on a volume that has a trash. A share without one, or a volume
    /// nobody could ask, must not read "you can undo this" (QA on an SMB Downloads cleanup).
    #[test]
    fn trash_is_undoable_only_where_the_source_volume_has_a_trash() {
        let stored = Reversibility::RestoreMove;
        assert_eq!(
            reversibility_on_source(ProposalVerb::Trash, stored, Some(true)),
            Reversibility::RestoreMove
        );
        assert_eq!(
            reversibility_on_source(ProposalVerb::Trash, stored, Some(false)),
            Reversibility::Irreversible
        );
        assert_eq!(
            reversibility_on_source(ProposalVerb::Trash, stored, None),
            Reversibility::Irreversible,
            "an unknown volume earns no promise"
        );
    }

    /// Every other verb's answer is the intent's own, whatever the volume can do.
    #[test]
    fn other_verbs_keep_the_intents_answer() {
        for (verb, stored) in [
            (ProposalVerb::Move, Reversibility::RestoreMove),
            (ProposalVerb::Copy, Reversibility::DeleteWhatWasWritten),
            (ProposalVerb::Delete, Reversibility::Irreversible),
            (ProposalVerb::Rename, Reversibility::RestoreMove),
        ] {
            assert_eq!(reversibility_on_source(verb, stored, Some(false)), stored, "{verb:?}");
            assert_eq!(reversibility_on_source(verb, stored, None), stored, "{verb:?}");
        }
    }

    #[test]
    fn a_shared_parent_is_named_only_when_every_path_has_it() {
        assert_eq!(
            shared_parent(["/Users/someone/Downloads/a.dmg", "/Users/someone/Downloads/b.zip"]).as_deref(),
            Some("/Users/someone/Downloads")
        );
        assert_eq!(
            shared_parent(["/Users/someone/Downloads/a.dmg", "/Users/someone/Desktop/b.zip"]),
            None
        );
        assert_eq!(
            shared_parent(["/Users/someone/Downloads/a.dmg", "/Users/someone/Downloads/sub/b.zip"]),
            None,
            "a file one level down sits in a different folder"
        );
        assert_eq!(shared_parent(Vec::<String>::new()), None);
    }

    /// Present files carry their facts, a missing one says so, and a stat the volume couldn't
    /// answer is unknown: three answers the dialog words differently, so none may collapse.
    #[tokio::test(flavor = "multi_thread")]
    async fn each_source_gets_the_answer_its_volume_gives() {
        let volume = InMemoryVolume::new("Test");
        volume
            .create_file(Path::new("/here.txt"), b"twelve bytes")
            .await
            .expect("seed file");
        volume.set_modified_at(Path::new("/here.txt"), Some(1_700_000_000));
        volume
            .create_file(Path::new("/unanswerable.txt"), b"x")
            .await
            .expect("seed file");
        volume.set_stat_failing(Path::new("/unanswerable.txt"));
        let volume: Arc<dyn Volume> = Arc::new(volume);

        let deadline = stat_deadline(volume.as_ref());
        let stats = stat_sources(
            volume,
            vec!["/here.txt".into(), "/gone.txt".into(), "/unanswerable.txt".into()],
            &deadline,
        )
        .await;

        assert!(
            matches!(
                stats[0],
                SourceStat::Present {
                    size: Some(12),
                    modified: Some(1_700_000_000),
                    ..
                }
            ),
            "{stats:?}"
        );
        assert_eq!(stats[1], SourceStat::Missing);
        assert_eq!(stats[2], SourceStat::Unknown);
        assert_eq!(
            stats[1].snapshot(),
            None,
            "a missing file freezes as NULL, never a zero"
        );
    }

    /// A spent deadline answers at once rather than queuing work nobody will read.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_spent_deadline_answers_unknown_without_asking() {
        let volume = InMemoryVolume::new("Test");
        volume.create_file(Path::new("/a.txt"), b"a").await.expect("seed file");
        let volume: Arc<dyn Volume> = Arc::new(volume);

        let stats = stat_sources(volume, vec!["/a.txt".into()], &Deadline::new(Duration::ZERO)).await;

        assert_eq!(stats, vec![SourceStat::Unknown]);
    }
}
