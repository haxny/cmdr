//! Log free-form text from outside Cmdr (OS, server, or CLI output) in full, as one field.
//!
//! Print it as `detail={:?}` (or `stderr={:?}` / `stdout={:?}`) with a [`LogDetail`]. The local
//! log keeps the text, capped at [`LOCAL_DETAIL_MAX_BYTES`], because it's often the only window
//! into what a server or tool actually said. The error reporter knows those three keys: a
//! report redacts the value with that report's context and caps it much shorter. That
//! redaction lives in the app's `redact` module, so producers never redact at the log site.
//!
//! When the text echoes values the caller handed the OS or tool (the path it opened, the share it
//! asked for), [`LogDetail::marking`] marks each occurrence as its kind, so a report hashes them
//! exactly instead of guessing. [`LoggedText`] is the sibling for text Cmdr itself already
//! logged, whose markers are its own.
//!
//! The Debug quoting is load-bearing: it keeps the value on one line and gives the report pass
//! an exact end quote, whatever the text contains. Every marker inside lands one `{:?}` depth
//! down, which the report pass reads natively.

use crate::log_private::{DebugEscaping, LogPrivate, Marker, MarkerRole, is_reserved, markers, write_debug_body};
use std::borrow::Cow;
use std::fmt::{self, Write as _};

/// Local-log cap for one field, in bytes of the raw text.
pub const LOCAL_DETAIL_MAX_BYTES: usize = 1024;

/// External text for one `detail=` / `stderr=` / `stdout=` log field. Its `Debug` prints the
/// text quoted and escaped, cut at [`LOCAL_DETAIL_MAX_BYTES`] on a char boundary with a
/// `…[N bytes]` marker naming the full length.
///
/// Every log-marker char in the text (`cmdr_fs::log_private::RESERVED`) prints as U+FFFD: the
/// text isn't ours, so a marker in it can only be a hostile name echoed back, and once quoted it
/// would spell a real marker.
#[derive(Clone, Copy)]
pub struct LogDetail<'a>(pub &'a str);

impl<'a> LogDetail<'a> {
    /// The same field with every occurrence of `values` in the text marked as its kind: the values
    /// the caller handed the OS, server, or tool, which its output tends to echo.
    ///
    /// The longest value claims first, so a file inside a marked dir stays one span. An occurrence
    /// glued to a letter or digit is part of another word and stays text; so does one spelled
    /// differently (NFD, shell-quoted), which the report's own redaction and echo scrub cover.
    /// The cap never cuts a span: it cuts before one it would split.
    pub fn marking<'v>(self, values: &'v [LogPrivate<'v>]) -> MarkedLogDetail<'a, 'v> {
        MarkedLogDetail { text: self.0, values }
    }
}

impl fmt::Debug for LogDetail<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_field(f, self.0.trim(), &[])
    }
}

/// A [`LogDetail`] with the caller's values marked; built by [`LogDetail::marking`].
#[derive(Clone, Copy)]
pub struct MarkedLogDetail<'a, 'v> {
    text: &'a str,
    values: &'v [LogPrivate<'v>],
}

impl fmt::Debug for MarkedLogDetail<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.text.trim();
        write_field(f, text, &occurrences(text, self.values))
    }
}

/// Text Cmdr itself already logged (a `log_error!` message), quoted as one field like
/// [`LogDetail`], with its markers KEPT: they're our own log sites' marks, so each span goes one
/// depth down and keeps hashing exactly. A stray marker an unmarked site let through stays a
/// stray, which the report pass fails closed on. The cap never cuts a span.
#[derive(Clone, Copy)]
pub struct LoggedText<'a>(pub &'a str);

impl fmt::Debug for LoggedText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.0.trim();
        let end = (text.len() > LOCAL_DETAIL_MAX_BYTES).then(|| cut_outside_spans(text, floor_char_boundary(text)));
        f.write_char('"')?;
        write_debug_body(f, &text[..end.unwrap_or(text.len())])?;
        if end.is_some() {
            write!(DebugEscaping(f), "…[{} bytes]", text.len())?;
        }
        f.write_char('"')
    }
}

/// One occurrence of a caller's value in the text.
struct Occurrence<'v> {
    start: usize,
    end: usize,
    value: LogPrivate<'v>,
}

/// Write `text` quoted, with each occurrence as its marked value and every reserved char outside
/// them as U+FFFD, capped without splitting an occurrence.
fn write_field(f: &mut fmt::Formatter<'_>, text: &str, found: &[Occurrence<'_>]) -> fmt::Result {
    let cut = (text.len() > LOCAL_DETAIL_MAX_BYTES).then(|| {
        let at = floor_char_boundary(text);
        found
            .iter()
            .find(|o| o.start < at && at < o.end)
            .map_or(at, |o| o.start)
    });
    let end = cut.unwrap_or(text.len());
    f.write_char('"')?;
    let mut pos = 0;
    for occurrence in found.iter().take_while(|o| o.end <= end) {
        write_foreign(f, &text[pos..occurrence.start])?;
        write!(DebugEscaping(f), "{}", occurrence.value)?;
        pos = occurrence.end;
    }
    write_foreign(f, &text[pos..end])?;
    if cut.is_some() {
        write!(DebugEscaping(f), "…[{} bytes]", text.len())?;
    }
    f.write_char('"')
}

/// Text that isn't ours, as a `{:?}` body with every reserved char replaced by U+FFFD.
fn write_foreign(out: &mut dyn fmt::Write, text: &str) -> fmt::Result {
    for (index, piece) in text.split(is_reserved).enumerate() {
        if index > 0 {
            out.write_char('\u{FFFD}')?;
        }
        write_debug_body(out, piece)?;
    }
    Ok(())
}

/// Where each value occurs in `text`, longest value first, never overlapping, left to right.
/// Both sides are compared with their reserved chars replaced, the way the text gets written,
/// so a hostile name still finds its own echo. A reserved char and U+FFFD are both three UTF-8
/// bytes, so offsets carry over to the original text unchanged.
fn occurrences<'v>(text: &str, values: &[LogPrivate<'v>]) -> Vec<Occurrence<'v>> {
    let haystack = without_reserved(text);
    let mut needles: Vec<(String, LogPrivate<'v>)> = values
        .iter()
        .map(|value| (without_reserved(&value.text()).into_owned(), *value))
        .filter(|(needle, _)| !needle.is_empty())
        .collect();
    needles.sort_by_key(|(needle, _)| std::cmp::Reverse(needle.len()));

    let mut found: Vec<Occurrence<'v>> = Vec::new();
    for (needle, value) in &needles {
        for (start, _) in haystack.match_indices(needle.as_str()) {
            let end = start + needle.len();
            let glued_before = haystack[..start].chars().next_back().is_some_and(char::is_alphanumeric);
            let glued_after = haystack[end..].chars().next().is_some_and(char::is_alphanumeric);
            let overlaps = found.iter().any(|o| start < o.end && o.start < end);
            if !glued_before && !glued_after && !overlaps {
                found.push(Occurrence {
                    start,
                    end,
                    value: *value,
                });
            }
        }
    }
    found.sort_by_key(|o| o.start);
    found
}

fn without_reserved(text: &str) -> Cow<'_, str> {
    if text.contains(is_reserved) {
        Cow::Owned(text.replace(is_reserved, "\u{FFFD}"))
    } else {
        Cow::Borrowed(text)
    }
}

/// [`LOCAL_DETAIL_MAX_BYTES`], moved back to a char boundary of `text`.
fn floor_char_boundary(text: &str) -> usize {
    let mut end = LOCAL_DETAIL_MAX_BYTES.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// `cut`, or the start of the span it would split: of the span still open there, or of a marker
/// the cut falls inside. Read with the same lexer and pairing rule (a closer at its opener's
/// depth) as the report pass.
fn cut_outside_spans(text: &str, cut: usize) -> usize {
    let mut open: Option<Marker> = None;
    for marker in markers(text) {
        if marker.start >= cut {
            break;
        }
        if marker.end > cut {
            return open.map_or(marker.start, |o| o.start);
        }
        match (open, marker.role) {
            (None, MarkerRole::Open(_)) => open = Some(marker),
            (Some(o), MarkerRole::Close) if marker.depth == o.depth => open = None,
            _ => {}
        }
    }
    open.map_or(cut, |o| o.start)
}

#[cfg(test)]
#[path = "log_detail_test.rs"]
mod tests;
