//! Results (⌥⏎): the preview as a text file the user edits in their editor, and
//! the edited names read back.
//!
//! One line per row, `old name<TAB>new name`. A row is matched back by its OLD
//! name (composed, since an editor may recompose it), never by its line number,
//! so a file that appeared in the folder meanwhile can't shift a name onto the
//! wrong row. A line whose old name isn't in the folder any more, or that has no
//! tab, is skipped; the preview shows what came of each row.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::ignore_poison::IgnorePoison;

use super::plan::PreviewRow;

/// One name the user typed for a row, keyed by the row's name in the folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NameEdit {
    pub old_name: String,
    pub new_name: String,
}

/// The edits by composed old name, as the preview looks them up.
#[derive(Debug, Default)]
pub struct NameEdits(HashMap<String, String>);

impl NameEdits {
    pub fn new(edits: &[NameEdit]) -> Self {
        Self(
            edits
                .iter()
                .map(|e| (e.old_name.nfc().collect(), e.new_name.clone()))
                .collect(),
        )
    }

    /// The name the user typed for `old_name`, if any.
    pub fn get(&self, old_name: &str) -> Option<&str> {
        if self.0.is_empty() {
            return None;
        }
        let key: String = old_name.nfc().collect();
        self.0.get(&key).map(String::as_str)
    }
}

/// Why the edited names couldn't be read back.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum NamesFileError {
    /// No Results file was written in this session.
    NoFile,
    /// The file is gone or unreadable; `detail` is log text only.
    Unreadable { detail: String },
}

/// The file Results wrote last, which reading back reads. Never a path the
/// frontend names.
static WRITTEN: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The file's text for `rows`.
pub fn render(rows: &[PreviewRow]) -> String {
    let mut text = String::new();
    for row in rows {
        text.push_str(&row.old_name);
        text.push('\t');
        text.push_str(&row.new_name);
        text.push('\n');
    }
    text
}

/// The edits in `text`. The new name is everything after a line's LAST tab, so
/// an old name holding a tab still splits right; surrounding blanks the editor
/// added are trimmed off the new name.
pub fn parse(text: &str) -> Vec<NameEdit> {
    text.lines()
        .filter_map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let (old, new) = line.rsplit_once('\t')?;
            (!old.is_empty()).then(|| NameEdit {
                old_name: old.to_string(),
                new_name: new.trim().to_string(),
            })
        })
        .collect()
}

/// Writes `rows` to the Results file in `dir` and remembers it for reading back.
pub fn write(dir: &Path, rows: &[PreviewRow]) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("multi-rename-names.txt");
    std::fs::write(&path, render(rows))?;
    *WRITTEN.lock_ignore_poison() = Some(path.clone());
    Ok(path)
}

/// The edits in the file Results wrote last.
pub fn read_back() -> Result<Vec<NameEdit>, NamesFileError> {
    let path = WRITTEN.lock_ignore_poison().clone().ok_or(NamesFileError::NoFile)?;
    let text = std::fs::read_to_string(&path).map_err(|e| NamesFileError::Unreadable { detail: e.to_string() })?;
    Ok(parse(&text))
}

/// Where Results writes: the user's temp folder.
pub fn default_dir() -> PathBuf {
    std::env::temp_dir().join("cmdr-multi-rename")
}
