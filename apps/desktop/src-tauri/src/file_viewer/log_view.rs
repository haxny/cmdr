//! The debug log's view of a text session: every row shows a marked value as itself, its log
//! markers gone (`crate::redact::display_line`), while the file on disk keeps them for the error
//! reporter. The view applies where rows leave a backend, so search columns, highlights, wrap,
//! copy, and "save range" all work on the text the user sees; byte offsets stay the raw file's.

use super::search_matcher::Matcher;
use super::{
    BackendCapabilities, FileEncoding, FileViewerBackend, LineChunk, SearchMatch, SeekTarget, TotalRows, ViewerError,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// How a session shows its rows. Picked at open by the caller that knows what the file is
/// (Help > View debug log), ❌ never by sniffing the content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RowText {
    /// The file's text as decoded.
    #[default]
    Raw,
    /// Cmdr's own log: log markers hidden, each marked value shown as itself.
    LogDisplay,
}

impl RowText {
    /// One row's text as this view shows it.
    pub fn render(self, text: &str) -> Cow<'_, str> {
        match self {
            RowText::Raw => Cow::Borrowed(text),
            RowText::LogDisplay => crate::redact::display_line(text),
        }
    }

    /// Every row of `chunk` as this view shows it.
    pub fn apply(self, chunk: &mut LineChunk) {
        if self == RowText::Raw {
            return;
        }
        for row in &mut chunk.rows {
            if let Cow::Owned(shown) = self.render(&row.text) {
                row.text = shown;
            }
        }
    }
}

/// A backend whose rows come out through a [`RowText`]. The session wraps its current backend
/// on every read (`ViewerSession::load_backend`), so a backend swap (the index upgrade, an
/// encoding switch, a tail extend) keeps the view without any swap site knowing about it.
pub(super) struct ViewedBackend {
    pub(super) inner: Arc<Box<dyn FileViewerBackend>>,
    pub(super) rows: RowText,
}

impl FileViewerBackend for ViewedBackend {
    fn get_lines(&self, target: &SeekTarget, count: usize, cancel: &AtomicBool) -> Result<LineChunk, ViewerError> {
        let mut chunk = self.inner.get_lines(target, count, cancel)?;
        self.rows.apply(&mut chunk);
        Ok(chunk)
    }

    fn extend_to_boxed(&self, new_size: u64, cancel: &AtomicBool) -> Result<Box<dyn FileViewerBackend>, ViewerError> {
        self.inner.extend_to_boxed(new_size, cancel)
    }

    fn with_encoding(&self, new_encoding: FileEncoding) -> Option<Box<dyn FileViewerBackend>> {
        self.inner.with_encoding(new_encoding)
    }

    /// The matcher already reads rows the way the session shows them (`search_start` gives it
    /// the session's view), so the wrapped backend's walk serves the search as is.
    fn search(
        &self,
        matcher: &Matcher,
        cancel: &AtomicBool,
        matches: &Mutex<Vec<SearchMatch>>,
        progress: &Mutex<u64>,
    ) -> Result<u64, ViewerError> {
        self.inner.search(matcher, cancel, matches, progress)
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.inner.capabilities()
    }

    fn total_bytes(&self) -> u64 {
        self.inner.total_bytes()
    }

    fn total_rows(&self) -> TotalRows {
        self.inner.total_rows()
    }

    fn total_lines(&self) -> Option<usize> {
        self.inner.total_lines()
    }

    fn file_name(&self) -> &str {
        self.inner.file_name()
    }
}
