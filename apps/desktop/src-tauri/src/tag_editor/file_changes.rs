//! What a write would change, per file: the read-only list the Write dialog shows.

use std::path::PathBuf;

use cmdr_tags::ContentKind;
use cmdr_tags::shared::changes as engine_changes;

use super::removals::{Removals, removals_of};
use super::session::{FileChangeView, TagEditorError, with_session};
use super::view::file_name_of;

/// What a write as `kind` would change, per file, in current order: the fields, cover, and
/// extras the edits change, and what the kind removes beyond them (an MP3's conductor under
/// an audiobook write), read from each file after the lock, so the list matches the write.
pub fn changes(session_id: &str, kind: ContentKind) -> Result<Vec<FileChangeView>, TagEditorError> {
    let listed: Vec<(PathBuf, FileChangeView)> = with_session(session_id, |layout, session| {
        Ok(layout
            .tracks
            .iter()
            .filter_map(|path| {
                let loaded = session.by_path.get(path)?.loaded()?;
                let change = engine_changes(std::slice::from_ref(&loaded.disk), std::slice::from_ref(&loaded.edited))
                    .into_iter()
                    .next()?;
                let view = FileChangeView {
                    path: path.to_string_lossy().into_owned(),
                    file_name: file_name_of(&layout.dir, path),
                    fields: change.fields,
                    cover: change.cover,
                    extras: change.extras,
                    removes: Vec::new(),
                    removes_unknown: false,
                };
                Some((path.clone(), view))
            })
            .collect())
    })?;
    Ok(listed
        .into_iter()
        .map(|(path, view)| match removals_of(&path, kind) {
            Removals::Known(removes) => FileChangeView { removes, ..view },
            Removals::Unknown => FileChangeView {
                removes_unknown: true,
                ..view
            },
        })
        .collect())
}
