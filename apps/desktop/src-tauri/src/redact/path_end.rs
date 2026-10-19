//! Where a path found in prose really ends. The path branches over-match on purpose (spaces are legal in
//! labels and filenames), and this gives the trailing prose back. `DETAILS.md` § "Finding the end of a path".

use super::fields::starts_with_field_key;
use super::names::ends_with_unicode_escape;
use super::paths::{ends_filename, ends_sentence, has_extension_like_suffix};

/// Split a greedy path capture into (path, trailing_noise). The regex matches spaces inside
/// paths so both multi-word labels (`/Volumes/My Backup Drive/...`) and multi-word filenames
/// (`Invoice for Acme Corp.pdf`) land whole; that also sweeps up trailing English text like
/// `... .png now` or `... .rs:42:5`. We pull back the tail here before the rewriter runs,
/// then re-emit the tail verbatim in the dispatch output.
///
/// ❗ **This is the only thing standing between a filename and an uploaded bundle.** The
/// regex deliberately over-matches, so a boundary rule that gives back too much doesn't
/// merely lose prose, it publishes the part of the name it handed back. Both directions have
/// bitten: an over-eager capture once truncated 98 reports at `/Volumes/<volume>`, and an
/// over-cautious one shipped ` at 01.13.03 PM-2.jpeg` verbatim.
///
/// Trimmed, in order:
/// - everything from the first `": "`, the `{path}: {message}` seam nearly every caller
///   formats with. macOS forbids `:` in a filename, so this can't cut a real name short.
/// - everything after the first token that ENDS the path: one carrying a letter-led
///   extension (`report.pdf`, `PM-2.jpeg`) or ending a sentence (`naspi-1)`, `state.`). Only a
///   token past the last separator qualifies, a bare number or initial with a dot (`12.`,
///   `J.`) opens a name instead of ending a sentence, and a bracket closing a pair its own
///   segment opened (`[§ 12 AB]`, `(final)`) is part of the name. A `key=` word ends it before
///   that word.
/// - trailing sentence-ending punctuation (`.`, `,`, `;`, `!`, `?`, and an unpaired `)`, `]`, `}`)
/// - trailing `:<digits>` groups (line/column markers like `:42:5`)
/// - with `trim_prose_run`, a trailing RUN of space-separated words that are lowercase-initial
///   AND carry no extension (` failed to open`). The run never eats into the first segment after
///   the last `/`, which is what keeps `/Volumes/naspi and then it failed` down to `naspi`. It
///   never runs when the path reaches the seam, closing brackets in between included: the
///   seam already said where it ends.
///
/// Callers pass `trim_prose_run: false` when something else already marks the end: a keyed
/// field's value is a path by declaration, and so is a path filling its quotes. There a
/// lowercase last word (`Medical records`) is part of the name: trimming it split one folder
/// into a token plus a bare word, and gave the same folder a different token than its quoted
/// spelling on the next line.
pub(super) fn split_trailing_noise(s: &str, trim_prose_run: bool) -> (&str, &str) {
    let bytes = s.as_bytes();
    let mut end = bytes.len();

    // The `{path}: {message}` seam. Everything from it belongs to the message.
    let seam = s.find(": ");
    if let Some(seam) = seam {
        end = seam;
    }

    // Left to right: the first token that ends a filename or ends a sentence is the last
    // thing that can belong to the path. Scanning forwards is what separates
    // `report.pdf for alice@example.com` (cut after `report.pdf`) from
    // `Screenshot 2026-09-04 at 01.13.03 PM-2.jpeg` (no letter-led extension until the very
    // end, so the whole name stays). A backwards scan can't tell those apart: the email's
    // `.com` looks exactly like a filename extension from the right.
    //
    // Nothing before the last separator can end the path: a deeper segment after a token
    // proves the path goes on, whatever punctuation the token ends with. That keeps
    // `12. Summer trip/beach.jpg` and `Dr. Dre/track.mp3` whole. A match never holds ` /` (a
    // space before a slash ends it), so a later separator belongs to the same path, unless
    // its word starts a new reference (`and https://…`).
    {
        let separator = separator_of(s);
        let last_separator = s[..same_path_end(s, end)].rfind(separator);
        let mut offset = 0usize;
        for token in s[..end].split(' ') {
            let token_end = offset + token.len();
            let can_end = last_separator.is_none_or(|sep| token_end > sep);
            // A `key=` word (`(source_is_dir=true,`) is the next field, so the path ended
            // before it. Without this, a name's own `(final)` no longer ending the path would
            // carry it into the field list.
            if can_end && offset > 0 && starts_with_field_key(token.trim_start_matches(['(', '[', '{'])) {
                end = offset - 1;
                break;
            }
            if can_end
                && !token.is_empty()
                && (ends_filename(token)
                    || (ends_sentence(token)
                        && !opens_a_name(token, separator)
                        && !closes_a_pair(s, token_end, separator)))
            {
                end = token_end;
                break;
            }
            offset = token_end + 1; // step over the space
        }
    }

    // First: trim sentence-ending punctuation, one at a time.
    end = trim_closing_punctuation(s, end);

    // Repeatedly strip `:<digits>` suffixes (e.g. `:42`, `:42:5`).
    loop {
        let mut i = end;
        // consume digits from the right
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        if i < end && i > 0 && bytes[i - 1] == b':' {
            end = i - 1;
        } else {
            break;
        }
    }

    // Trim a RUN of trailing lowercase, extension-less words (a sentence continuation).
    //
    // Not when the path runs right up to the seam: then the seam already marks its end, and
    // a lowercase last word is part of a name (`/Volumes/x/summer trip: failed`). Trimming it
    // there shipped `trip` verbatim.
    //
    // The floor is the first word after the last `/`: that word is a real path segment
    // however lowercase it looks, so `/Volumes/naspi and then it failed` keeps `naspi`.
    // An extension ends the run on the spot, because a word carrying one is part of the
    // filename, not prose — that is what holds `my secret notes.txt` together.
    // Closing brackets in between don't hide the seam: `(under …/Summer trip): disk full` still
    // runs up to it, and trimming there shipped `trip`.
    let reaches_seam = seam.is_some_and(|seam| s[end..seam].bytes().all(|b| matches!(b, b')' | b']' | b'}')));
    if trim_prose_run && !reaches_seam {
        let floor = s[..end].rfind(separator_of(s)).map_or(0, |i| i + 1);
        loop {
            let mut i = end;
            while i > floor && bytes[i - 1] != b' ' {
                i -= 1;
            }
            // `i` is the start of the last word; require a space before it, and stay
            // above the floor so we never eat the segment itself.
            if i <= floor || i == end || bytes[i - 1] != b' ' {
                break;
            }
            let word = &s[i..end];
            let starts_lower = word.chars().next().is_some_and(|c| c.is_ascii_lowercase());
            if !starts_lower || has_extension_like_suffix(word) || looks_like_name_fragment(word) {
                break;
            }
            end = i - 1;
            while end > floor && bytes[end - 1] == b' ' {
                end -= 1;
            }
        }
    }

    // A lone trailing `-` is the start of a `{from} -> {to}` arrow (the regex stops at `>`).
    // After the prose run, so the `-` has already stopped it from eating the name's last word.
    if s[..end].ends_with(" -") {
        end -= 2;
    }

    // Finally, strip a trailing `.` or `,` that was exposed by the above steps.
    end = trim_closing_punctuation(s, end);

    // SAFETY: we only advance `end` on ASCII byte boundaries.
    (&s[..end], &s[end..])
}

/// Step `end` back over closing punctuation (`.` `,` `;` `!` `?` `)` `]` `}`), one at a time.
/// The `}` closing a `\u{301}` escape stays: it's the middle of a `{:?}`-printed name, and so
/// does a bracket that closes a pair its own segment opened (`Report (final)`, `Case [A 1]`).
fn trim_closing_punctuation(s: &str, mut end: usize) -> usize {
    let bytes = s.as_bytes();
    let separator = separator_of(s);
    while end > 0 {
        let b = bytes[end - 1];
        if b == b'}' && ends_with_unicode_escape(&s[..end]) {
            break;
        }
        if closes_a_pair(s, end, separator) {
            break;
        }
        if matches!(b, b'.' | b',' | b';' | b'!' | b'?' | b')' | b']' | b'}') {
            end -= 1;
        } else {
            break;
        }
    }
    end
}

/// Whether `s[..end]` ends in a closing bracket that its own segment opened: `[§ 12 AB]` and
/// `(final)` are part of a folder's name, while the `)` of `(pane sent to /Volumes/x/y)` was
/// opened before the path began. Reading the first as a sentence end shipped the rest of the
/// name after the `]` (ERR-8MEX6).
fn closes_a_pair(s: &str, end: usize, separator: char) -> bool {
    let (open, close) = match s.as_bytes().get(end.wrapping_sub(1)) {
        Some(b')') => ('(', ')'),
        Some(b']') => ('[', ']'),
        Some(b'}') => ('{', '}'),
        _ => return false,
    };
    let segment = &s[s[..end].rfind(separator).map_or(0, |i| i + 1)..end];
    segment.matches(open).count() >= segment.matches(close).count()
}

/// The character a path's segments are separated by: `\` for a Windows home (`C:\Users\…`) or
/// a UNC path (`\\host\share\…`), `/` for everything else. On a `/` path a `\` is a `{:?}`
/// escape, never a separator.
fn separator_of(s: &str) -> char {
    let bytes = s.as_bytes();
    let windows_drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    if windows_drive || s.starts_with("\\\\") {
        '\\'
    } else {
        '/'
    }
}

/// Where the separators that can belong to this path stop: before the first later word that
/// starts a new reference (`https://u:p@a.com and https://x:y@b.com`).
fn same_path_end(s: &str, end: usize) -> usize {
    let mut offset = 0usize;
    for (i, word) in s[..end].split(' ').enumerate() {
        if i > 0 && word.contains("://") {
            return offset;
        }
        offset += word.len() + 1;
    }
    end
}

/// A word that ends in a dot but opens a name rather than closing a sentence: a bare number
/// (`12.` of `12. Summer trip`, `1.` of `Chapter 1. Intro`) or a lone initial (`J.` of
/// `J. Smith`). A path rarely ends a sentence on one of those, and when it does, the cost is a
/// word of prose redacted; cutting there ships the rest of the folder's name.
fn opens_a_name(token: &str, separator: char) -> bool {
    let word = token.rsplit(separator).next().unwrap_or(token);
    let Some(stem) = word.strip_suffix('.') else {
        return false;
    };
    let all_digits = !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit());
    let initial = stem.len() == 1 && stem.bytes().all(|b| b.is_ascii_uppercase());
    all_digits || initial
}

/// A word prose doesn't produce: an inner dot (`me\u{301}retek.jpg.cmdr-tmp-…`, whose
/// extension is too odd for [`has_extension_like_suffix`]) or a `{:?}` escape. Such a word
/// is the tail of a filename, and trimming it as prose ships it verbatim.
fn looks_like_name_fragment(word: &str) -> bool {
    word.contains('\\') || word.find('.').is_some_and(|i| i > 0 && i + 1 < word.len())
}
