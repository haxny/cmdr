//! Container clocks: a video's UTC capture time turned into the local wall-clock
//! time every other date in a name uses.

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeDelta, TimeZone};

/// Turns a UTC instant into local wall-clock time. Injected so the readers' tests
/// don't depend on the machine's time zone; [`system_local`] in production.
pub(crate) type ToLocal<'a> = &'a dyn Fn(NaiveDateTime) -> NaiveDateTime;

/// `utc` on this machine's clock, with the offset in force at that instant
/// (summer time included).
pub(crate) fn system_local(utc: NaiveDateTime) -> NaiveDateTime {
    chrono::Local.from_utc_datetime(&utc).naive_local()
}

/// Midnight UTC on `year-01-01`, a container clock's epoch.
pub(crate) fn epoch(year: i32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, 1, 1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .expect("January 1st at midnight exists in every year a container uses")
}

/// `epoch` plus `delta`, when that's a plausible capture time. A clock of zero is
/// "never set", and one before 1970 or past 2199 is a wrong epoch or garbage,
/// never a real recording: both are absent.
pub(crate) fn container_time(epoch: NaiveDateTime, delta: TimeDelta) -> Option<NaiveDateTime> {
    if delta.is_zero() {
        return None;
    }
    let time = epoch.checked_add_signed(delta)?;
    (1970..=2199).contains(&time.year()).then_some(time)
}
