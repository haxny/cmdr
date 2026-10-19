//! The span pass: values a producer marked (`cmdr_fs::log_private`) are hashed exactly, by the
//! kind the producer gave them, before the regex fallback reads the line. Broken and hostile
//! markers fail closed. The rules and the escape-depth decision: `DETAILS.md` § "Marked spans".
//!
//! The pass hands the regex a line with one placeholder per span (chars from the reader scratch
//! range), so the fallback treats a span as an ordinary path chunk it can neither split on nor
//! re-read, and [`Spans::fill`] puts the tokens in afterwards. The sinks that show a log to a
//! person read the same markers through [`display_line`].

use super::RedactionContext;
use super::context::TokenDomain;
use super::detail::{EchoedIdentity, is_name_token};
use super::fields::redact_typed_path;
use super::identity_token;
use super::names::unescape_debug;
use super::path_end::split_trailing_noise;
use super::paths::{Leaf, redact_leaf};
use super::references::redact_host;
use cmdr_fs::log_private::{self, LogKind, MarkerRole, READER_SCRATCH, is_reserved};
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// What a broken span (an opener with no closer, a closer with no opener) becomes.
const BROKEN_KIND: &str = "redacted";

/// Leads every placeholder; the index follows as [`PLACEHOLDER_DIGITS`].
const PLACEHOLDER_LEAD: char = *READER_SCRATCH.end();
/// The ten chars a placeholder writes its index in, `0` first.
const PLACEHOLDER_DIGITS: char = *READER_SCRATCH.start();

/// A line's spans: the line with a placeholder where each one was, and what replaces each.
pub(super) struct Spans {
    pub(super) line: String,
    replacements: Vec<String>,
    /// The values the spans name, for the echo scrub of the rest of the line.
    echoes: Vec<EchoedIdentity>,
}

impl Spans {
    /// No spans: what an unmarked line runs with.
    pub(super) const NONE: Spans = Spans {
        line: String::new(),
        replacements: Vec::new(),
        echoes: Vec::new(),
    };

    /// The identities the spans name, for the line's echo scrub.
    pub(super) fn echoes(&self) -> &[EchoedIdentity] {
        &self.echoes
    }

    /// Put each span's token where its placeholder survived the scan. A placeholder the scan
    /// folded into a bigger name is gone with that name's hash, which is what it should be.
    pub(super) fn fill<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.replacements.is_empty() || !text.contains(PLACEHOLDER_LEAD) {
            return Cow::Borrowed(text);
        }
        let mut out = String::with_capacity(text.len() + 32 * self.replacements.len());
        let mut rest = text;
        while let Some(at) = rest.find(PLACEHOLDER_LEAD) {
            out.push_str(&rest[..at]);
            let after = &rest[at + PLACEHOLDER_LEAD.len_utf8()..];
            let digits: String = after.chars().map_while(placeholder_digit).collect();
            let used: usize = digits.len() * PLACEHOLDER_DIGITS.len_utf8();
            let replacement = digits
                .parse::<usize>()
                .ok()
                .and_then(|index| self.replacements.get(index));
            // Only this pass writes the scratch range into a line, so a miss can't happen; if
            // it ever does, nothing about the value is known, so nothing of it is written.
            out.push_str(replacement.map_or("<redacted>", String::as_str));
            rest = &after[used..];
        }
        out.push_str(rest);
        Cow::Owned(out)
    }
}

/// The regex for one placeholder, for the path branches that may start at one.
pub(super) fn placeholder_pattern() -> String {
    let last_digit = char::from_u32(PLACEHOLDER_DIGITS as u32 + 9).unwrap_or(PLACEHOLDER_DIGITS);
    format!(
        r"\x{{{:X}}}[\x{{{:X}}}-\x{{{:X}}}]+",
        PLACEHOLDER_LEAD as u32, PLACEHOLDER_DIGITS as u32, last_digit as u32
    )
}

/// Whether `value` is exactly one placeholder: a span the scan must leave for [`Spans::fill`].
/// Only the span pass writes these; typed values and span values reach the walkers with every
/// reserved char replaced ([`without_reserved`]), so nothing else can spell one.
pub(super) fn is_placeholder(value: &str) -> bool {
    value
        .strip_prefix(PLACEHOLDER_LEAD)
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| placeholder_digit(c).is_some()))
}

/// `value` with every reserved char replaced by U+FFFD, so a value that held one (a hostile
/// name) can never pass for a marker or a placeholder further on.
pub(super) fn without_reserved(value: &str) -> Cow<'_, str> {
    if value.chars().any(is_reserved) {
        Cow::Owned(
            value
                .chars()
                .map(|c| if is_reserved(c) { '\u{FFFD}' } else { c })
                .collect(),
        )
    } else {
        Cow::Borrowed(value)
    }
}

fn placeholder(index: usize) -> String {
    let mut out = String::from(PLACEHOLDER_LEAD);
    for digit in index.to_string().bytes() {
        out.push(char::from_u32(PLACEHOLDER_DIGITS as u32 + u32::from(digit - b'0')).unwrap_or(PLACEHOLDER_DIGITS));
    }
    out
}

fn placeholder_digit(c: char) -> Option<char> {
    let offset = (c as u32).checked_sub(PLACEHOLDER_DIGITS as u32)?;
    (offset < 10).then(|| char::from(b'0' + offset as u8))
}

/// Read the line's spans, or `None` for a line with no marker (nearly every line, at the cost of
/// two byte scans).
pub(super) fn read_spans(line: &str, context: Option<&RedactionContext>) -> Option<Spans> {
    if !may_hold_markers(line) {
        return None;
    }
    let markers = log_private::markers(line);
    if markers.is_empty() {
        return None;
    }

    let fields = quoted_values(line);
    let enclosing = |at: usize| fields.iter().copied().find(|&(open, close)| open <= at && at < close);
    let line_end = line.trim_end_matches(['\n', '\r']).len();
    let mut boundary = message_start(line);
    let mut regions: Vec<(usize, usize, String)> = Vec::new();
    let mut echoes = Vec::new();
    let mut next = 0;

    while next < markers.len() {
        let marker = markers[next];
        let field = enclosing(marker.start);
        // Never short of the marker itself, so every slice below is well-formed.
        let limit = field.map_or(line_end, |(_, close)| close).max(marker.end);
        let (start, end, replacement) = match marker.role {
            MarkerRole::Open(kind) => {
                let closer = markers[next + 1..]
                    .iter()
                    .find(|m| m.role == MarkerRole::Close && m.depth == marker.depth && m.end <= limit);
                match closer {
                    Some(closer) => {
                        let value = decode(&line[marker.end..closer.start], marker.depth);
                        let replacement = redact_value(kind, &value, context);
                        echoes.extend(echo_of(kind, &value, &replacement));
                        (marker.start, closer.end, replacement)
                    }
                    // Rule 2: no closer before the line (or the field) ends.
                    None => (marker.start, limit, broken(&line[marker.start..limit], context)),
                }
            }
            // Rule 3: a closer nothing opened. The text before it back to the last thing we know
            // ends a value, and the name glued after it, are the damage.
            MarkerRole::Close => {
                let back = field.map_or(boundary, |(open, _)| open.max(boundary)).min(marker.start);
                let forward_limit = markers.get(next + 1).map_or(limit, |m| m.start.min(limit));
                let end = marker.end + stray_tail(&line[marker.end..forward_limit], field.is_some());
                (back, end, broken(&line[back..end], context))
            }
        };
        regions.push((start, end, replacement));
        boundary = end;
        while next < markers.len() && markers[next].start < boundary {
            next += 1;
        }
    }

    let mut rewritten = String::with_capacity(line.len());
    let mut replacements = Vec::with_capacity(regions.len());
    let mut pos = 0;
    for (index, (start, end, replacement)) in regions.into_iter().enumerate() {
        rewritten.push_str(&line[pos..start]);
        rewritten.push_str(&placeholder(index));
        replacements.push(replacement);
        pos = end;
    }
    rewritten.push_str(&line[pos..]);
    Some(Spans {
        line: rewritten,
        replacements,
        echoes,
    })
}

/// How far past a stray closer its damage runs: to a quote, or to where the greedy prose boundary
/// ends a path (`split_trailing_noise`). Inside a quoted field the field's own end bounds it.
fn stray_tail(after: &str, in_field: bool) -> usize {
    let hard = if in_field {
        after.len()
    } else {
        after.find('"').unwrap_or(after.len())
    };
    split_trailing_noise(&after[..hard]).0.len()
}

/// A span's value: its text unescaped once per `{:?}` layer, plus the body escaping every span
/// carries, with any reserved char left in it replaced so it can't pass for structure later.
fn decode(text: &str, depth: u32) -> String {
    let mut value = Cow::Borrowed(text);
    for _ in 0..=depth {
        if !value.contains('\\') {
            break;
        }
        value = Cow::Owned(unescape_debug(&value).into_owned());
    }
    without_reserved(&value).into_owned()
}

/// The token a value of `kind` gets: the same typed calls a report's typed values use.
fn redact_value(kind: LogKind, value: &str, context: Option<&RedactionContext>) -> String {
    match kind {
        LogKind::Path => redact_typed_path(value, context, false, Leaf::Guess),
        LogKind::FilePath => redact_typed_path(value, context, false, Leaf::File),
        LogKind::DirPath => redact_typed_path(value, context, false, Leaf::Dir),
        LogKind::Name => redact_leaf(value, Leaf::Guess.is_file(value), context),
        LogKind::FileName => redact_leaf(value, true, context),
        LogKind::DirName => redact_leaf(value, false, context),
        LogKind::Volume => identity_token("volume", TokenDomain::Volume, value, context),
        LogKind::Host => redact_host(value, context),
        LogKind::User => identity_token("user", TokenDomain::Userinfo, value, context),
    }
}

/// The identity a span names, for the echo scrub: a path's or name's leaf (scrubbed from prose
/// too), or an identity (scrubbed from external text, like a keyed `host=`).
fn echo_of(kind: LogKind, value: &str, replacement: &str) -> Option<EchoedIdentity> {
    let last = |s: &str| s.rsplit(['/', '\\']).find(|seg| !seg.is_empty()).map(str::to_string);
    match kind {
        LogKind::Path | LogKind::FilePath | LogKind::DirPath | LogKind::Name | LogKind::FileName | LogKind::DirName => {
            let token = last(replacement)?;
            is_name_token(&token)
                .then(|| EchoedIdentity::new(last(value)?, token, true))
                .flatten()
        }
        LogKind::Volume | LogKind::Host | LogKind::User => {
            EchoedIdentity::new(value.to_string(), replacement.to_string(), false)
        }
    }
}

fn broken(text: &str, context: Option<&RedactionContext>) -> String {
    identity_token(BROKEN_KIND, TokenDomain::Path, text, context)
}

/// Whether the line may hold a marker: the UTF-8 lead bytes every reserved char shares, or a
/// `u{` that could open an escaped one.
fn may_hold_markers(line: &str) -> bool {
    let bytes = line.as_bytes();
    memchr::memmem::find(bytes, b"\xEF\xB7").is_some() || memchr::memmem::find(bytes, b"u{").is_some()
}

/// Where the message starts in a log line (`<ts> LEVEL [·]target  message`), or 0 for text with no
/// header. A stray closer may erase the message, never the header the bundle filters read, and the
/// echo scrub leaves the header alone too (a folder named `2026` isn't the timestamp's year).
pub(super) fn message_start(line: &str) -> usize {
    let Some(stamp_end) = line
        .find(' ')
        .filter(|_| line.starts_with(|c: char| c.is_ascii_digit()))
    else {
        return 0;
    };
    let after_stamp = &line[stamp_end + 1..];
    let Some(level) = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]
        .iter()
        .find(|level| after_stamp.starts_with(*level))
    else {
        return 0;
    };
    let mut rest = after_stamp[level.len()..].trim_start_matches(' ');
    // The optional RAM gauge, `(1.2 GB) `, sits before the target.
    if rest.starts_with('(')
        && let Some(close) = rest.find(") ")
    {
        rest = &rest[close + 2..];
    }
    let Some(gap) = rest.find("  ").filter(|&gap| !rest[..gap].contains(' ')) else {
        return 0;
    };
    line.len() - rest.len() + gap + 2
}

/// The value ranges of the line's quoted fields (`key="…"`, `key: "…"`, `key=Some("…")`): from just
/// after the opening quote to the closing quote. A cut span inside one ends at its quote.
fn quoted_values(line: &str) -> Vec<(usize, usize)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"\b[A-Za-z_][A-Za-z0-9_]*(?:=|:\x20)(?:Some\()?"(?P<value>(?:[^"\\\n]|\\.)*)""#)
            .expect("valid quoted-field regex")
    });
    re.captures_iter(line)
        .filter_map(|caps| caps.name("value").map(|value| (value.start(), value.end())))
        .collect()
}

/// A marked line as a person reads it: no markers, and each span's value shown as itself
/// (`Café`, not `Cafe\u{301}`), with only what could break the display escaped (control and
/// bidi-format chars). For the terminal and the in-app debug log viewer. Reserved chars never
/// survive, raw or escaped.
pub(crate) fn display_line(line: &str) -> Cow<'_, str> {
    if !may_hold_markers(line) {
        return Cow::Borrowed(line);
    }
    let markers = log_private::markers(line);
    if markers.is_empty() {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len());
    let mut pos = 0;
    let mut next = 0;
    while next < markers.len() {
        let marker = markers[next];
        out.push_str(&line[pos..marker.start]);
        let closer = match marker.role {
            MarkerRole::Open(_) => markers[next + 1..]
                .iter()
                .position(|m| m.role == MarkerRole::Close && m.depth == marker.depth)
                .map(|offset| next + 1 + offset),
            MarkerRole::Close => None,
        };
        match closer {
            Some(closer) => {
                push_for_display(
                    &mut out,
                    &decode(&line[marker.end..markers[closer].start], marker.depth),
                );
                pos = markers[closer].end;
                next = closer + 1;
            }
            None => {
                pos = marker.end;
                next += 1;
            }
        }
    }
    out.push_str(&line[pos..]);
    // A value can hold literal marker text; take that out too, until nothing is left of one.
    loop {
        let markers = log_private::markers(&out);
        if markers.is_empty() {
            break;
        }
        let mut stripped = String::with_capacity(out.len());
        let mut pos = 0;
        for marker in markers {
            stripped.push_str(&out[pos..marker.start]);
            pos = marker.end;
        }
        stripped.push_str(&out[pos..]);
        out = stripped;
    }
    Cow::Owned(out)
}

/// `value` as itself, with control and bidi-format chars escaped the `{:?}` way so a name can't
/// split the line or reorder it. [`decode`] already replaced any reserved char.
fn push_for_display(out: &mut String, value: &str) {
    for c in value.chars() {
        if c.is_control() || is_bidi_format(c) {
            out.extend(c.escape_debug());
        } else {
            out.push(c);
        }
    }
}

/// The chars that reorder how a line displays (U+202A–U+202E, U+2066–U+2069, and the marks).
fn is_bidi_format(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}
