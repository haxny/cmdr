//! The user metadata Cmdr writes on objects.
//!
//! S3 keeps only upload time, so the source file's mtime travels in
//! `x-amz-meta-mtime`, in rclone's key and format (`backend/s3/s3.go`'s
//! `metaMtime`, written by `swift.TimeToFloatString` from `ncw/swift`'s
//! `meta.go`; read on 2026-10-01): Unix seconds as a fixed-point decimal with
//! up to nine fractional digits and trailing zeros dropped, like
//! `1354040105.123456789`. Matching it means Cmdr and rclone read each other's
//! dates.
//!
//! The creation date and mode travel in rclone's `--metadata` keys too (`btime`
//! and `mode`, from `backend/local/metadata.go`; `backend/s3/s3.go` writes
//! `btime` as user metadata "since we can't set it" and every other key as is;
//! read at rclone `4f19e9c`, 2026-10-09): `btime` in RFC 3339 with nanoseconds,
//! `mode` as the whole `st_mode` in octal, like `100644`. Together with the
//! mtime and a write token they stay near 150 bytes of S3's 2 KB user-metadata
//! budget, so a copy never nears it on its own (`DETAILS.md` § "Creation date
//! and mode").

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// The header carrying the mtime. S3 stores metadata keys lowercase.
pub(crate) const MTIME_HEADER: &str = "x-amz-meta-mtime";

/// The header carrying the creation date (rclone's `btime`).
pub(crate) const BTIME_HEADER: &str = "x-amz-meta-btime";

/// The header carrying the mode (rclone's `mode`).
pub(crate) const MODE_HEADER: &str = "x-amz-meta-mode";

/// The header naming the one PUT that wrote an object (`write_token`), so a
/// write that was cut off can recognise and remove what a server kept of it
/// (`volume/writes.rs` § "A cut-off PUT"), ❌ and nothing else.
pub(crate) const WRITE_TOKEN_HEADER: &str = "x-amz-meta-cmdr-write";

/// A token no other write on this machine uses: the process, the clock, and a
/// counter. Not a secret, only an identity for one PUT.
pub(crate) fn write_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!(
        "{:x}-{nanos:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// `at` in rclone's format.
pub(crate) fn format_mtime(at: SystemTime) -> String {
    match at.duration_since(UNIX_EPOCH) {
        Ok(after) => fixed_point(after.as_nanos()),
        Err(before) => format!("-{}", fixed_point(before.duration().as_nanos())),
    }
}

/// An mtime in rclone's format, or `None` when it isn't one. Digits only,
/// an optional leading `-` and one `.`; a fraction past nine digits is cut,
/// a shorter one padded, the way rclone reads it.
pub(crate) fn parse_mtime(text: &str) -> Option<SystemTime> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let secs: u64 = whole.parse().ok()?;
    let mut digits: String = fraction.chars().take(9).collect();
    while digits.len() < 9 {
        digits.push('0');
    }
    let offset = Duration::new(secs, digits.parse().ok()?);
    if negative {
        UNIX_EPOCH.checked_sub(offset)
    } else {
        UNIX_EPOCH.checked_add(offset)
    }
}

/// `at` as rclone's `btime`: RFC 3339 in UTC, the fraction only when there is
/// one. `None` for a date RFC 3339 can't say (past the year 9999).
pub(crate) fn format_btime(at: SystemTime) -> Option<String> {
    OffsetDateTime::from(at).format(&Rfc3339).ok()
}

/// A `btime` in RFC 3339, in any offset (rclone's local backend writes the
/// machine's zone), or `None` when it isn't one.
pub(crate) fn parse_btime(text: &str) -> Option<SystemTime> {
    OffsetDateTime::parse(text, &Rfc3339).ok().map(SystemTime::from)
}

/// A file's permission bits as rclone's `mode`: the whole `st_mode` of a
/// regular file, in octal.
pub(crate) fn format_mode(mode: u32) -> String {
    format!("{:o}", S_IFREG | (mode & 0o7777))
}

/// The permission bits of an rclone `mode`, or `None` when it isn't octal.
pub(crate) fn parse_mode(text: &str) -> Option<u32> {
    u32::from_str_radix(text, 8).ok().map(|mode| mode & 0o7777)
}

/// `S_IFREG`, the file-type bits rclone's `mode` carries for a file.
const S_IFREG: u32 = 0o100_000;

/// Nanoseconds as seconds with up to nine decimals, trailing zeros dropped.
fn fixed_point(nanos: u128) -> String {
    let secs = nanos / NANOS_PER_SEC;
    let fraction = nanos % NANOS_PER_SEC;
    if fraction == 0 {
        return secs.to_string();
    }
    let digits = format!("{fraction:09}");
    format!("{secs}.{}", digits.trim_end_matches('0'))
}

const NANOS_PER_SEC: u128 = 1_000_000_000;

#[cfg(test)]
#[path = "metadata_test.rs"]
mod metadata_test;
