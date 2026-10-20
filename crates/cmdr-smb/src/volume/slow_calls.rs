//! The `warn` for a metadata round trip (a stat, a listing) that took the
//! server more than a second.
//!
//! A busy QNAP holds 0.5–3% of single metadata requests for 0.3–6 s while its
//! session stays healthy, and nothing else in a log shows it: smb2 rightly
//! treats it as slow rather than dead, so no error, reconnect, or timeout fires.
//! One line per stall would flood the log on a bad NAS, so the lines roll up
//! per share (`cmdr_fs::log_rollup`) and each rolled-up line names the slowest
//! call it stands for. ❌ No path in the line: the share and the call are what a
//! reader needs to place it, and the path is what a bundle shouldn't carry.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::log_rollup::LogRollup;

/// A metadata call that takes longer than this is the server holding it: a
/// healthy one answers in milliseconds.
const SLOW_CALL: Duration = Duration::from_secs(1);

/// At most one line per share per this window once stalls keep coming.
const ROLLUP_WINDOW: Duration = Duration::from_secs(60);

/// Decides whether a metadata call is worth a line, and words it.
pub(super) struct SlowCallLog {
    rollup: LogRollup,
    /// Per share, the slowest call folded into the next line.
    slowest: Mutex<Option<HashMap<String, Duration>>>,
}

impl SlowCallLog {
    pub(super) const fn new() -> Self {
        Self {
            rollup: LogRollup::new(ROLLUP_WINDOW),
            slowest: Mutex::new(None),
        }
    }

    /// The line to log for `op` on `share` having taken `elapsed`, or `None`
    /// when it was quick or folds into a later line.
    pub(super) fn line_at(&self, share: &str, op: &str, elapsed: Duration, now: Instant) -> Option<SlowCallLine> {
        if elapsed < SLOW_CALL {
            return None;
        }
        // One guard across the rollup's decision, so a stall on another thread
        // can't land between reading the slowest and resetting it.
        let (batch, slowest) = {
            let mut guard = self.slowest.lock_ignore_poison();
            let per_share = guard.get_or_insert_with(HashMap::new);
            let slowest = per_share.entry(share.to_string()).or_default();
            *slowest = (*slowest).max(elapsed);
            let slowest = *slowest;
            let batch = self.rollup.record_at(share, now)?;
            per_share.remove(share);
            (batch, slowest)
        };

        Some(SlowCallLine {
            share: share.to_string(),
            op: op.to_string(),
            took: elapsed,
            rollup: batch.is_rolled_up().then_some(Rollup {
                count: batch.count,
                over: batch.elapsed,
                slowest,
            }),
        })
    }
}

/// One slow-call `warn` line. A `Display` rather than a `String`, so the share
/// goes out marked (`LogPrivate`) and the marked text never becomes data.
#[derive(Debug, PartialEq)]
pub(super) struct SlowCallLine {
    share: String,
    op: String,
    took: Duration,
    /// Set when the line stands for every stall since the last one.
    rollup: Option<Rollup>,
}

#[derive(Debug, PartialEq)]
struct Rollup {
    count: u64,
    over: Duration,
    slowest: Duration,
}

impl fmt::Display for SlowCallLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let share = LogPrivate::volume(&self.share);
        let op = &self.op;
        let took = self.took.as_millis();
        match &self.rollup {
            Some(rollup) => write!(
                f,
                "SMB metadata calls were slow: share={share:?} ×{} over {} s in {}s, slowest {} ms, latest {op} {took} ms. \
                 The server held them; check its load (a busy NAS does this under heavy writes)",
                rollup.count,
                SLOW_CALL.as_secs(),
                rollup.over.as_secs(),
                rollup.slowest.as_millis(),
            ),
            None => write!(
                f,
                "SMB metadata call was slow: share={share:?} {op} took {took} ms. \
                 The server held it; check its load (a busy NAS does this under heavy writes)"
            ),
        }
    }
}

static SLOW_CALLS: SlowCallLog = SlowCallLog::new();

/// Logs `op` on `share` at `warn` when the server held it, rolled up per share.
pub(super) fn note(share: &str, op: &str, elapsed: Duration) {
    if let Some(line) = SLOW_CALLS.line_at(share, op, elapsed, Instant::now()) {
        log::warn!("{line}");
    }
}

#[cfg(test)]
#[path = "slow_calls_test.rs"]
mod slow_calls_test;
