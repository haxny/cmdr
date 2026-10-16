//! The rate limit between a refusal and the person: pure state over an injected clock, so a
//! misbehaving client can't turn into a toast storm and the rule is testable without an app.
//!
//! Keyed by the notice itself, which carries its source (the server's volume id, the offered
//! version, or nothing for MCP, where a local process can't be told apart). The first refusal
//! for a key raises with a count of one; later ones bump the count and re-raise at most once per
//! window, and a count that grew inside a window is raised once the window ends ([`due`]), so
//! the toast never stops short of the real number.
//!
//! [`due`]: NoticeGate::due

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::AccessNotice;

/// What the gate lets through: tell the person about `notice`, seen `count` times this session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raise {
    pub notice: AccessNotice,
    pub count: u64,
}

struct Seen {
    count: u64,
    /// The count the last raise carried.
    raised_count: u64,
    raised_at: Instant,
}

pub struct NoticeGate {
    window: Duration,
    seen: HashMap<AccessNotice, Seen>,
}

impl NoticeGate {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            seen: HashMap::new(),
        }
    }

    /// One more refusal for `notice`. Answers the raise, or `None` while the window holds it.
    pub fn note(&mut self, notice: AccessNotice, now: Instant) -> Option<Raise> {
        match self.seen.get_mut(&notice) {
            None => {
                self.seen.insert(
                    notice.clone(),
                    Seen {
                        count: 1,
                        raised_count: 1,
                        raised_at: now,
                    },
                );
                Some(Raise { notice, count: 1 })
            }
            Some(seen) => {
                seen.count += 1;
                if now.saturating_duration_since(seen.raised_at) < self.window {
                    return None;
                }
                seen.raised_count = seen.count;
                seen.raised_at = now;
                Some(Raise {
                    notice,
                    count: seen.count,
                })
            }
        }
    }

    /// The counts that grew since their last raise and whose window has ended.
    pub fn due(&mut self, now: Instant) -> Vec<Raise> {
        let window = self.window;
        let mut due = Vec::new();
        for (notice, seen) in &mut self.seen {
            if seen.count > seen.raised_count && now.saturating_duration_since(seen.raised_at) >= window {
                seen.raised_count = seen.count;
                seen.raised_at = now;
                due.push(Raise {
                    notice: notice.clone(),
                    count: seen.count,
                });
            }
        }
        due
    }

    /// When the earliest held count comes due, or `None` when nothing is held.
    pub fn next_due(&self) -> Option<Instant> {
        self.seen
            .values()
            .filter(|seen| seen.count > seen.raised_count)
            .map(|seen| seen.raised_at + self.window)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(60);

    fn origin() -> AccessNotice {
        AccessNotice::McpForeignOrigin
    }

    fn smb(volume_id: &str) -> AccessNotice {
        AccessNotice::SmbSignatureRefused {
            volume_id: volume_id.into(),
        }
    }

    fn raise(notice: AccessNotice, count: u64) -> Option<Raise> {
        Some(Raise { notice, count })
    }

    #[test]
    fn the_first_refusal_raises_with_a_count_of_one() {
        let mut gate = NoticeGate::new(WINDOW);
        assert_eq!(gate.note(origin(), Instant::now()), raise(origin(), 1));
    }

    #[test]
    fn a_thousand_refusals_in_a_minute_raise_once_then_once_more_with_the_full_count() {
        let mut gate = NoticeGate::new(WINDOW);
        let t0 = Instant::now();
        assert_eq!(gate.note(origin(), t0), raise(origin(), 1));
        for i in 1..1_000u64 {
            assert_eq!(gate.note(origin(), t0 + Duration::from_millis(i * 50)), None);
        }
        assert_eq!(gate.next_due(), Some(t0 + WINDOW));
        assert!(
            gate.due(t0 + WINDOW - Duration::from_secs(1)).is_empty(),
            "not a minute yet"
        );
        assert_eq!(
            gate.due(t0 + WINDOW),
            vec![Raise {
                notice: origin(),
                count: 1_000
            }]
        );
        assert!(gate.due(t0 + WINDOW * 3).is_empty(), "nothing new to say");
        assert_eq!(gate.next_due(), None);
    }

    #[test]
    fn a_refusal_after_the_window_raises_at_once() {
        let mut gate = NoticeGate::new(WINDOW);
        let t0 = Instant::now();
        gate.note(origin(), t0);
        assert_eq!(gate.note(origin(), t0 + WINDOW), raise(origin(), 2));
        assert_eq!(gate.note(origin(), t0 + WINDOW + Duration::from_secs(1)), None);
    }

    #[test]
    fn a_raise_restarts_the_window() {
        let mut gate = NoticeGate::new(WINDOW);
        let t0 = Instant::now();
        gate.note(origin(), t0);
        gate.note(origin(), t0 + Duration::from_secs(10));
        assert_eq!(
            gate.due(t0 + WINDOW),
            vec![Raise {
                notice: origin(),
                count: 2
            }]
        );
        gate.note(origin(), t0 + WINDOW + Duration::from_secs(1));
        assert!(gate.due(t0 + WINDOW + Duration::from_secs(59)).is_empty());
        assert_eq!(
            gate.due(t0 + WINDOW * 2),
            vec![Raise {
                notice: origin(),
                count: 3
            }]
        );
    }

    #[test]
    fn each_source_raises_its_own() {
        let mut gate = NoticeGate::new(WINDOW);
        let t0 = Instant::now();
        assert_eq!(gate.note(smb("smb-nas"), t0), raise(smb("smb-nas"), 1));
        assert_eq!(gate.note(smb("smb-other"), t0), raise(smb("smb-other"), 1));
        assert_eq!(gate.note(origin(), t0), raise(origin(), 1));
        assert_eq!(gate.note(smb("smb-nas"), t0), None);
    }
}
