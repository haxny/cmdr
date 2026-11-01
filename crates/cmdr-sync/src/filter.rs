//! Total Commander's sync masks: which names take part in a synchronization.

use cmdr_fs::name_fold::fold_name;

/// The exclude list a profile starts with: the junk the OS scatters around.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    ".DS_Store",
    "._*",
    ".Trashes/",
    ".Spotlight-V100/",
    ".fseventsd/",
    "Thumbs.db",
];

/// The excludes that answer [`Admission::Junk`] instead of [`Admission::Out`]:
/// files nobody put there on purpose, which travel with their folder.
pub(crate) const JUNK_MASKS: &[&str] = &[".DS_Store", "._*", "Thumbs.db"];

/// A folder Synchronize folders keeps its own state in. Out at every level, and
/// no setting switches it off.
pub(crate) const ALWAYS_EXCLUDED: &str = ".cmdr-sync";

/// Whether `name` is the app-owned folder. The scanner leaves it out silently,
/// uncounted, at every level; [`MaskFilter::admits`] still answers `Out` for it.
pub(crate) fn is_app_owned(name: &str) -> bool {
    fold_name(name) == fold_name(ALWAYS_EXCLUDED)
}

/// Why a mask list can't be used. (Named for the feature: Multi-Rename has a `MaskError` of its own
/// and both reach the one bindings file.) `position` counts characters of the text
/// that held the problem (for an entry of the exclude list, its index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SyncMaskError {
    /// A mask with nothing in it: `""`, or a `|` with nothing after it.
    EmptyMask {
        /// Where the empty mask sits.
        position: u32,
    },
    /// A `"` that never closes.
    UnclosedQuote {
        /// Where the quote opens.
        position: u32,
    },
    /// More than one `|`.
    SecondBar {
        /// Where the second bar sits.
        position: u32,
    },
}

impl std::fmt::Display for SyncMaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMask { position } => write!(f, "empty mask at {position}"),
            Self::UnclosedQuote { position } => write!(f, "quote opened at {position} never closes"),
            Self::SecondBar { position } => write!(f, "second `|` at {position}"),
        }
    }
}

impl std::error::Error for SyncMaskError {}

/// What the filter says about one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Takes part.
    In,
    /// Left out on purpose. Counted as held back.
    Out,
    /// OS junk: not an entry, and not counted as held back.
    Junk,
}

#[derive(Debug, Clone)]
struct Mask {
    /// Folded pattern without its leading or trailing `/`.
    glob: Vec<char>,
    folders_only: bool,
    /// Matches the whole path from the root, not just the name.
    whole_path: bool,
    junk: bool,
}

impl Mask {
    fn new(text: &str) -> Self {
        // TC masks pasted from Windows use backslashes.
        let folded = fold_name(&text.replace('\\', "/")).into_owned();
        let anchored = folded.starts_with('/');
        let folders_only = folded.len() > 1 && folded.ends_with('/');
        let core = folded.trim_start_matches('/').trim_end_matches('/');
        let junk = JUNK_MASKS.iter().any(|m| fold_name(m) == fold_name(text));
        Self {
            glob: core.chars().collect(),
            folders_only,
            whole_path: anchored || core.contains('/'),
            junk,
        }
    }

    fn matches(&self, folded_path: &str, is_dir: bool) -> bool {
        if self.folders_only && !is_dir {
            return false;
        }
        let subject = if self.whole_path {
            folded_path
        } else {
            folded_path.rsplit('/').next().unwrap_or(folded_path)
        };
        glob_matches(&self.glob, &subject.chars().collect::<Vec<_>>())
    }
}

/// `*` and `?` over characters, with backtracking to the last `*`.
fn glob_matches(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut resume: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                resume = Some((p, t));
                p += 1;
            }
            Some(c) if *c == '?' || *c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match resume {
                Some((star, at)) => {
                    p = star + 1;
                    t = at + 1;
                    resume = Some((star, at + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// A parsed include/exclude mask list.
#[derive(Debug, Clone)]
pub struct MaskFilter {
    includes: Vec<Mask>,
    excludes: Vec<Mask>,
}

impl MaskFilter {
    /// Parses TC sync syntax: masks split by spaces or `;`, `"…"` keeps spaces,
    /// at most one `|` starts the excludes. `excludes` (the editable list) is
    /// added after the bar. An empty include part admits everything.
    pub fn parse(masks: &str, excludes: &[String]) -> Result<Self, SyncMaskError> {
        let (include_tokens, exclude_tokens) = split_masks(masks)?;
        let mut exclude_masks: Vec<Mask> = exclude_tokens.iter().map(|t| Mask::new(t)).collect();
        for (index, text) in excludes.iter().enumerate() {
            if text.trim().is_empty() {
                return Err(SyncMaskError::EmptyMask { position: index as u32 });
            }
            exclude_masks.push(Mask::new(text.trim()));
        }
        Ok(Self {
            includes: include_tokens.iter().map(|t| Mask::new(t)).collect(),
            excludes: exclude_masks,
        })
    }

    /// The verdict on one entry. `rel_path` is the entry's path from the sync
    /// root with `/` between components, in either side's own spelling. An
    /// excluded ancestor folder excludes everything under it.
    pub fn admits(&self, rel_path: &str, is_dir: bool) -> Admission {
        let folded = fold_name(rel_path).into_owned();
        let always = fold_name(ALWAYS_EXCLUDED);
        if folded.split('/').any(|part| part == always) {
            return Admission::Out;
        }
        let mut junk = false;
        let mut end = 0;
        for part in folded.split('/') {
            end += part.len();
            let prefix = &folded[..end];
            let prefix_is_dir = is_dir || end < folded.len();
            match self.excluded(prefix, prefix_is_dir) {
                Some(false) => return Admission::Out,
                Some(true) => junk = true,
                None => {}
            }
            end += 1;
        }
        if !is_dir && !self.includes.is_empty() && !self.includes.iter().any(|m| m.matches(&folded, false)) {
            return Admission::Out;
        }
        if junk { Admission::Junk } else { Admission::In }
    }

    /// `Some(false)` for a real exclude, `Some(true)` for junk only, `None` for no match.
    fn excluded(&self, folded_path: &str, is_dir: bool) -> Option<bool> {
        let mut hit: Option<bool> = None;
        for mask in self.excludes.iter().filter(|m| m.matches(folded_path, is_dir)) {
            if !mask.junk {
                return Some(false);
            }
            hit = Some(true);
        }
        hit
    }
}

/// Splits the box text into (includes, excludes) mask texts.
fn split_masks(text: &str) -> Result<(Vec<String>, Vec<String>), SyncMaskError> {
    let mut sides: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    let mut side = 0;
    let mut bar_position = None;
    let mut current = String::new();
    let mut quoted_from: Option<u32> = None;
    let mut had_quotes = false;
    let flush = |current: &mut String, had_quotes: &mut bool, side: usize, sides: &mut [Vec<String>; 2], at: u32| {
        if current.is_empty() && *had_quotes {
            return Err(SyncMaskError::EmptyMask { position: at });
        }
        if !current.is_empty() {
            sides[side].push(std::mem::take(current));
        }
        *had_quotes = false;
        Ok(())
    };
    for (index, c) in text.chars().enumerate() {
        let at = index as u32;
        if quoted_from.is_some() {
            if c == '"' {
                quoted_from = None;
            } else {
                current.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                quoted_from = Some(at);
                had_quotes = true;
            }
            ' ' | ';' | '\t' => flush(&mut current, &mut had_quotes, side, &mut sides, at)?,
            '|' => {
                flush(&mut current, &mut had_quotes, side, &mut sides, at)?;
                if bar_position.is_some() {
                    return Err(SyncMaskError::SecondBar { position: at });
                }
                bar_position = Some(at);
                side = 1;
            }
            _ => current.push(c),
        }
    }
    if let Some(position) = quoted_from {
        return Err(SyncMaskError::UnclosedQuote { position });
    }
    flush(
        &mut current,
        &mut had_quotes,
        side,
        &mut sides,
        text.chars().count() as u32,
    )?;
    let [includes, excludes] = sides;
    if let Some(position) = bar_position
        && excludes.is_empty()
    {
        return Err(SyncMaskError::EmptyMask { position });
    }
    Ok((includes, excludes))
}
