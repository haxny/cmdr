//! The placeholders the span pass (`markers.rs`) writes where each marked value was, spelled in
//! the reader scratch range so nothing else can write one. A leaf: the path rewriters ask
//! [`is_placeholder`] to leave a span alone, and the regex reads [`placeholder_pattern`].

use cmdr_fs::log_private::READER_SCRATCH;

/// Leads every placeholder; the index follows as [`PLACEHOLDER_DIGITS`].
pub(super) const PLACEHOLDER_LEAD: char = *READER_SCRATCH.end();
/// The ten chars a placeholder writes its index in, `0` first.
pub(super) const PLACEHOLDER_DIGITS: char = *READER_SCRATCH.start();

/// The regex for one placeholder, for the path branches that may start at one.
pub(super) fn placeholder_pattern() -> String {
    let last_digit = char::from_u32(PLACEHOLDER_DIGITS as u32 + 9).unwrap_or(PLACEHOLDER_DIGITS);
    format!(
        r"\x{{{:X}}}[\x{{{:X}}}-\x{{{:X}}}]+",
        PLACEHOLDER_LEAD as u32, PLACEHOLDER_DIGITS as u32, last_digit as u32
    )
}

/// Whether `value` is exactly one placeholder: a span the scan must leave for `Spans::fill`.
/// Only the span pass writes these; typed values and span values reach the walkers with every
/// reserved char replaced (`markers::without_reserved`), so nothing else can spell one.
pub(super) fn is_placeholder(value: &str) -> bool {
    value
        .strip_prefix(PLACEHOLDER_LEAD)
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| placeholder_digit(c).is_some()))
}

/// The placeholder for the span at `index`.
pub(super) fn placeholder(index: usize) -> String {
    let mut out = String::from(PLACEHOLDER_LEAD);
    for digit in index.to_string().bytes() {
        out.push(char::from_u32(PLACEHOLDER_DIGITS as u32 + u32::from(digit - b'0')).unwrap_or(PLACEHOLDER_DIGITS));
    }
    out
}

/// The decimal digit a placeholder char spells, or `None` for any other char.
pub(super) fn placeholder_digit(c: char) -> Option<char> {
    let offset = (c as u32).checked_sub(PLACEHOLDER_DIGITS as u32)?;
    (offset < 10).then(|| char::from(b'0' + offset as u8))
}
